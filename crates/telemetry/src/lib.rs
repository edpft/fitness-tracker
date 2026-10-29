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
//!
//! **Or timed by something else.** A check nix-fast-build ran, a test nextest
//! ran or a job CI ran was timed by that tool, and [`write`] records it with
//! the times it reports (#629).

use std::{
    path::{Path, PathBuf},
    thread::JoinHandle,
};

use jiff::Timestamp;
use rand::Rng as _;
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};
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
pub const NAMES: [&str; 8] = [
    "command", "request", "run", "step", "gate", "check", "test", "job",
];

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
        .spawn(move || writing(&database, receiver))
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

/// A span something else timed, with the spans that ran inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct Timed {
    /// One of [`NAMES`].
    pub name: &'static str,
    /// What identifies it wherever it came from, so that writing it twice
    /// leaves one row: a CI job's id, say. Without one it is a row each time.
    pub key: Option<String>,
    pub started_at: Timestamp,
    pub ended_at: Timestamp,
    pub outcome: Option<Outcome>,
    pub fields: Map<String, Value>,
    pub children: Vec<Self>,
}

/// Write `spans`, and the spans inside them, to the `telemetry.db` at
/// `database`, each stamped with `build`.
///
/// # Errors
///
/// If the file cannot be opened or a row cannot be written. Unlike a
/// [`Recorder`]'s, this is said: whoever imports a run wants to know it did
/// not land.
pub fn write(database: &Path, build: Option<&str>, spans: &[Timed]) -> Result<(), sqlx::Error> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut connection = connect(database).await?;
        for span in spans {
            written_with(&mut connection, None, build, span).await?;
        }
        connection.close().await
    })
}

/// Whether a span with `key` is already in the `telemetry.db` at `database`,
/// so that an import can pass over what it has.
///
/// # Errors
///
/// If the file cannot be opened or read.
pub fn is_written(database: &Path, key: &str) -> Result<bool, sqlx::Error> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut connection = connect(database).await?;
        let found = sqlx::query("SELECT 1 FROM span WHERE id = ?")
            .bind(keyed_id(key))
            .fetch_optional(&mut connection)
            .await?;
        connection.close().await?;
        Ok(found.is_some())
    })
}

async fn written_with(
    connection: &mut SqliteConnection,
    parent: Option<i64>,
    build: Option<&str>,
    span: &Timed,
) -> Result<(), sqlx::Error> {
    let id = span.key.as_deref().map_or_else(span_id, keyed_id);
    insert(
        connection,
        &Row {
            id,
            parent,
            name: span.name,
            build,
            started_at: span.started_at,
            ended_at: span.ended_at,
            outcome: span.outcome,
            fields: &span.fields,
        },
    )
    .await?;
    for child in &span.children {
        Box::pin(written_with(connection, Some(id), build, child)).await?;
    }
    Ok(())
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

/// The same for the same key, in any process and any build.
fn keyed_id(key: &str) -> i64 {
    let digest = Sha256::digest(key.as_bytes());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(digest.get(..8).unwrap_or_default());
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

    /// **A span that outlives what started it** names it with
    /// `follows_from`, since a parent stays open until its last child closes:
    /// a page load that starts a run would end when the run did (#628). One
    /// with no parent of its own is recorded as the child of what it follows
    /// from, which must still be open when it is named.
    fn on_follows_from(&self, id: &Id, follows: &Id, context: Context<'_, S>) {
        let Some(cause) = context
            .span(follows)
            .and_then(|cause| cause.extensions().get::<Open>().map(|open| open.id))
        else {
            return;
        };
        if let Some(span) = context.span(id)
            && let Some(open) = span.extensions_mut().get_mut::<Open>()
            && open.parent.is_none()
        {
            open.parent = Some(cause);
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
fn writing(database: &Path, spans: UnboundedReceiver<Closed>) {
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
            let _ = insert(
                connection,
                &Row {
                    id: span.id,
                    parent: span.parent,
                    name: span.name,
                    build: span.build.as_deref(),
                    started_at: span.started_at,
                    ended_at: span.ended_at,
                    outcome: span.outcome,
                    fields: &span.fields,
                },
            )
            .await;
        }
    }
    if let Some(connection) = connection {
        let _ = connection.close().await;
    }
}

async fn open(database: &Path) -> Option<SqliteConnection> {
    connect(database).await.ok()
}

async fn connect(database: &Path) -> Result<SqliteConnection, sqlx::Error> {
    let options = SqliteConnectOptions::new()
        .filename(database)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(std::time::Duration::from_secs(1));
    let mut connection = SqliteConnection::connect_with(&options).await?;
    // **A migration this build does not know is not an error.** The installed
    // build and `cargo xtask` from a working tree share the file, and the tree
    // is often a migration ahead: refusing it, the installed build recorded
    // nothing until it was upgraded (#629). So a migration here only ever
    // widens what a row may be, and an older build still writes its rows.
    let mut migrator = sqlx::migrate!();
    migrator.set_ignore_missing(true);
    migrator.run(&mut connection).await?;
    Ok(connection)
}

/// One row, however its span was timed.
struct Row<'a> {
    id: i64,
    parent: Option<i64>,
    name: &'a str,
    build: Option<&'a str>,
    started_at: Timestamp,
    ended_at: Timestamp,
    outcome: Option<Outcome>,
    fields: &'a Map<String, Value>,
}

// Unchecked at compile time, unlike the store's queries: `query!` checks
// against the one schema `.sqlx` is prepared from, and that is the store's.
// The suite in `tests/` writes and reads it back.
//
// `OR IGNORE` so that a keyed span written twice is one row.
async fn insert(connection: &mut SqliteConnection, span: &Row<'_>) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT OR IGNORE INTO span (id, parent, name, build, started_at, ended_at, outcome, fields)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(span.id)
    .bind(span.parent)
    .bind(span.name)
    .bind(span.build)
    .bind(span.started_at.to_string())
    .bind(span.ended_at.to_string())
    .bind(span.outcome.map(Outcome::as_str))
    .bind(Value::Object(span.fields.clone()).to_string())
    .execute(connection)
    .await
    .map(|_| ())
}
