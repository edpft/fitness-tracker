//! Informational telemetry (#631): each closed `tracing` span is a row in a
//! `telemetry.db` of its own, beside the store and never inside it.
//!
//! **Informational only.** Nothing the tool needs in order to work is read
//! from here, so a span that cannot be written is lost and nothing else: the
//! command being recorded neither fails nor says so.
//!
//! **Written off the command's path.** A [`Recorder`] hands each closed span to
//! a thread of its own, which opens the file at the first span and not before,
//! so a command that records nothing creates nothing. Dropping the
//! [`Recording`] waits for that thread to write what it was given.

use std::{
    path::{Path, PathBuf},
    thread::JoinHandle,
};

use jiff::Timestamp;
use rand::Rng as _;
use serde_json::{Map, Value};
use sqlx::{
    Connection as _, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, WeakUnboundedSender};
use tracing::{
    Subscriber,
    field::{Field, Visit},
    span::{Attributes, Id, Record},
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

/// The span names recorded, and the only ones: anything else — a dependency's
/// own instrumentation above all — is not this tool's telemetry.
pub const NAMES: [&str; 7] = ["command", "request", "run", "step", "gate", "check", "test"];

/// The field a span's outcome is recorded in, by [`Outcome::as_str`].
pub const OUTCOME: &str = "outcome";

/// How a span ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed,
    /// Recorded with what was found: not due, or unchanged.
    Skipped,
}

impl Outcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [Self::Succeeded, Self::Failed, Self::Skipped]
            .into_iter()
            .find(|outcome| outcome.as_str() == value)
    }
}

/// The layer: it times each recorded span and hands it on when it closes.
pub struct Recorder {
    // Weak, so that dropping the [`Recording`] closes the channel although the
    // global subscriber holding this layer never drops.
    spans: WeakUnboundedSender<Closed>,
    build: Option<String>,
}

/// The writing, until it is dropped: dropping it waits for every span already
/// closed to be written, or to fail to be.
pub struct Recording {
    spans: Option<UnboundedSender<Closed>>,
    writer: Option<JoinHandle<()>>,
}

impl Drop for Recording {
    fn drop(&mut self) {
        drop(self.spans.take());
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

/// Record spans to the `telemetry.db` at `database`, each stamped with `build`:
/// the commit, as `fitness --version` names it, or nothing for a build that
/// cannot name one.
pub fn record(database: &Path, build: Option<&str>) -> (Recorder, Recording) {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let database = database.to_path_buf();
    let writer = std::thread::Builder::new()
        .name("telemetry".into())
        .spawn(move || write(&database, receiver))
        .ok();
    (
        Recorder {
            spans: sender.downgrade(),
            build: build.map(str::to_owned),
        },
        Recording {
            spans: Some(sender),
            writer,
        },
    )
}

/// A span while it is open.
struct Open {
    id: i64,
    parent: Option<i64>,
    started_at: Timestamp,
    fields: Map<String, Value>,
}

/// A span once it has closed: one row.
struct Closed {
    id: i64,
    parent: Option<i64>,
    name: &'static str,
    build: Option<String>,
    started_at: Timestamp,
    ended_at: Timestamp,
    outcome: Option<Outcome>,
    fields: Map<String, Value>,
}

/// Unique across processes writing the same file, which a counter is not.
fn span_id() -> i64 {
    let mut bytes = [0_u8; 8];
    rand::rng().fill_bytes(&mut bytes);
    i64::from_le_bytes(bytes)
}

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for Recorder {
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        if !NAMES.contains(&attributes.metadata().name()) {
            return;
        }
        let Some(span) = context.span(id) else {
            return;
        };
        let parent = span
            .scope()
            .skip(1)
            .find_map(|ancestor| ancestor.extensions().get::<Open>().map(|open| open.id));
        let mut fields = Map::new();
        attributes.record(&mut Fields(&mut fields));
        span.extensions_mut().insert(Open {
            id: span_id(),
            parent,
            started_at: Timestamp::now(),
            fields,
        });
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, context: Context<'_, S>) {
        if let Some(span) = context.span(id)
            && let Some(open) = span.extensions_mut().get_mut::<Open>()
        {
            values.record(&mut Fields(&mut open.fields));
        }
    }

    fn on_close(&self, id: Id, context: Context<'_, S>) {
        let Some(span) = context.span(&id) else {
            return;
        };
        let Some(Open {
            id,
            parent,
            started_at,
            mut fields,
        }) = span.extensions_mut().remove::<Open>()
        else {
            return;
        };
        let outcome = match fields.remove(OUTCOME) {
            Some(Value::String(outcome)) => Outcome::parse(&outcome),
            _ => None,
        };
        if let Some(spans) = self.spans.upgrade() {
            let _ = spans.send(Closed {
                id,
                parent,
                name: span.name(),
                build: self.build.clone(),
                started_at,
                ended_at: Timestamp::now(),
                outcome,
                fields,
            });
        }
    }
}

/// A span's fields, as JSON.
struct Fields<'a>(&'a mut Map<String, Value>);

impl Visit for Fields<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.into());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_owned(), value.into());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_owned(), value.into());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_owned(), value.into());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_owned(), format!("{value:?}").into());
    }
}

/// The writer thread: open the file at the first span, then write each one.
/// A file that will not open is given up on, and what arrives after is
/// dropped.
fn write(database: &Path, spans: UnboundedReceiver<Closed>) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    runtime.block_on(written(database.to_path_buf(), spans));
}

async fn written(database: PathBuf, mut spans: UnboundedReceiver<Closed>) {
    let mut connection = None;
    let mut unwritable = false;
    while let Some(span) = spans.recv().await {
        if unwritable {
            continue;
        }
        if connection.is_none() {
            connection = open(&database).await;
            unwritable = connection.is_none();
        }
        if let Some(connection) = connection.as_mut() {
            let _ = insert(connection, &span).await;
        }
    }
    if let Some(connection) = connection {
        let _ = connection.close().await;
    }
}

async fn open(database: &Path) -> Option<SqliteConnection> {
    let options = SqliteConnectOptions::new()
        .filename(database)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(1));
    let mut connection = SqliteConnection::connect_with(&options).await.ok()?;
    sqlx::migrate!().run(&mut connection).await.ok()?;
    Some(connection)
}

// Unchecked at compile time, unlike the store's queries: `query!` checks
// against the one schema `.sqlx` is prepared from, and that is the store's.
// The suite in `tests/` writes and reads it back.
async fn insert(connection: &mut SqliteConnection, span: &Closed) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO span (id, parent, name, build, started_at, ended_at, outcome, fields)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(span.id)
    .bind(span.parent)
    .bind(span.name)
    .bind(span.build.as_deref())
    .bind(span.started_at.to_string())
    .bind(span.ended_at.to_string())
    .bind(span.outcome.map(Outcome::as_str))
    .bind(Value::Object(span.fields.clone()).to_string())
    .execute(connection)
    .await
    .map(|_| ())
}
