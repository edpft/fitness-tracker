//! What a closed span leaves in `telemetry.db` (#631).

use std::path::Path;

use sqlx::{Connection as _, Row as _, SqliteConnection, sqlite::SqliteConnectOptions};
use telemetry::{OUTCOME, Outcome};
use tracing::field::Empty;
use tracing_subscriber::layer::SubscriberExt as _;

struct Row {
    id: i64,
    parent: Option<i64>,
    name: String,
    build: Option<String>,
    started_at: String,
    ended_at: String,
    outcome: Option<String>,
    fields: serde_json::Value,
}

fn spans(database: &Path) -> Result<Vec<Row>, Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let options = SqliteConnectOptions::new().filename(database);
        let mut connection = SqliteConnection::connect_with(&options).await?;
        let rows = sqlx::query(
            "SELECT id, parent, name, build, started_at, ended_at, outcome, fields
             FROM span ORDER BY started_at",
        )
        .fetch_all(&mut connection)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(Row {
                    id: row.try_get("id")?,
                    parent: row.try_get("parent")?,
                    name: row.try_get("name")?,
                    build: row.try_get("build")?,
                    started_at: row.try_get("started_at")?,
                    ended_at: row.try_get("ended_at")?,
                    outcome: row.try_get("outcome")?,
                    fields: serde_json::from_str(row.try_get("fields")?)?,
                })
            })
            .collect()
    })
}

/// Run `body` with spans recorded to `database`, and wait for them to be
/// written.
fn recorded(database: &Path, build: Option<&str>, body: impl FnOnce()) {
    let (recorder, recording) = telemetry::record(database, build);
    let subscriber = tracing_subscriber::registry().with(recorder);
    tracing::subscriber::with_default(subscriber, body);
    drop(recording);
}

#[test]
fn a_closed_span_is_one_row_with_its_fields_timed_and_stamped() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("telemetry.db");

    recorded(&database, Some("abc1234 (2026-10-29)"), || {
        let span = tracing::info_span!(
            "step",
            step = "canonicalising",
            layer = "gym",
            read = Empty,
            written = Empty,
            outcome = Empty,
        );
        span.record("read", 12_u64);
        span.record("written", 9_u64);
        span.record(OUTCOME, Outcome::Succeeded.as_str());
    });

    let rows = spans(&database).unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row.name, "step");
    assert_eq!(row.parent, None);
    assert_eq!(row.build.as_deref(), Some("abc1234 (2026-10-29)"));
    assert_eq!(row.outcome.as_deref(), Some("succeeded"));
    assert_eq!(
        row.fields,
        serde_json::json!({
            "step": "canonicalising",
            "layer": "gym",
            "read": 12,
            "written": 9,
        })
    );
    let started: jiff::Timestamp = row.started_at.parse().unwrap();
    let ended: jiff::Timestamp = row.ended_at.parse().unwrap();
    assert!(started <= ended);
}

#[test]
fn a_step_is_the_child_of_the_run_it_was_in() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("telemetry.db");

    recorded(&database, None, || {
        let run = tracing::info_span!("run");
        let _inside = run.enter();
        // Not recorded, and not in the way: the step's parent is still the run.
        let other = tracing::info_span!("query");
        let _also_inside = other.enter();
        let _step = tracing::info_span!("step", step = "fetching");
    });

    let rows = spans(&database).unwrap();
    let run = rows.iter().find(|row| row.name == "run").unwrap();
    let step = rows.iter().find(|row| row.name == "step").unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(run.parent, None);
    assert_eq!(step.parent, Some(run.id));
    assert_eq!(step.build, None);
    assert_eq!(step.outcome, None);
}

/// **A run a page load started is that load's child, and the load ends when
/// it is served** (#628): the run follows from the load rather than sitting
/// inside it, which would hold the load open until the run ended.
#[test]
fn a_span_following_from_another_is_its_child_without_outliving_it() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("telemetry.db");

    recorded(&database, None, || {
        let request = tracing::info_span!("request");
        let run = tracing::info_span!(parent: None, "run");
        run.follows_from(&request);
        drop(request);
        std::thread::sleep(std::time::Duration::from_millis(20));
        drop(run);
    });

    let rows = spans(&database).unwrap();
    let request = rows.iter().find(|row| row.name == "request").unwrap();
    let run = rows.iter().find(|row| row.name == "run").unwrap();
    assert_eq!(run.parent, Some(request.id));
    let ended = |row: &Row| row.ended_at.parse::<jiff::Timestamp>().unwrap();
    assert!(ended(request) < ended(run));
}

#[test]
fn nothing_recorded_creates_no_file() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("telemetry.db");

    recorded(&database, None, || {
        let _query = tracing::info_span!("query");
        tracing::info!("an event, which is not a span");
    });

    assert!(!database.exists());
}

#[test]
fn a_file_that_cannot_be_written_loses_the_spans_and_nothing_else() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("telemetry.db");
    std::fs::create_dir(&database).unwrap();

    recorded(&database, None, || {
        let _step = tracing::info_span!("step");
    });

    assert!(database.is_dir());
}
