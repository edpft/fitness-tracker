//! Two writers landing at once, against a real SQLite file (#608).
//!
//! A run fetches its sources side by side, so one source's write can start
//! while another's still holds the store. A transaction that reads before it
//! writes must then wait for the lock. If it opened deferred, SQLite refuses its
//! first write outright, `database is locked`, without waiting out the busy
//! timeout, and that source's data is missing until the next run.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::{error::Error, path::Path, time::Duration};

use infrastructure::{SqlitePelotonClassStore, connect, peloton::ClassSummary};
use sqlx::{
    Connection as _, SqliteConnection,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};
use tempfile::TempDir;

type Fallible<T> = Result<T, Box<dyn Error>>;

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

fn listed(id: &str) -> ClassSummary {
    ClassSummary {
        id: id.to_owned(),
        title: "45 min Power Zone Ride".to_owned(),
        duration_seconds: 2700,
        series: Some("0f63c48726fa4533a928cae5358d94d7".to_owned()),
        instructor: Some("an-instructor".to_owned()),
        aired_at: Some(1_761_980_000),
    }
}

/// Another writer: a connection of its own that takes the write lock, writes,
/// and holds it for `held` before committing.
async fn another_writer(path: &Path, held: Duration) -> Fallible<()> {
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .journal_mode(SqliteJournalMode::Wal),
    )
    .await?;
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut connection)
        .await?;
    sqlx::query(
        "INSERT INTO peloton_class (reference, title, duration, listed_at) \
         VALUES ('held-by-another-writer', 'A ride', 1200, '2026-11-02T07:31:14Z')",
    )
    .execute(&mut connection)
    .await?;
    tokio::time::sleep(held).await;
    sqlx::query("COMMIT").execute(&mut connection).await?;
    Ok(())
}

/// 2026-11-02 07:31: the class catalogue counted its rows, then wrote, while
/// the Garmin walk held the store, and was refused. It now waits for the lock
/// and counts what the other writer committed meanwhile as not its own.
#[test]
fn a_write_waits_for_another_writer_instead_of_failing() {
    let Ok(rt) = runtime() else {
        panic!("a current-thread runtime builds")
    };
    rt.block_on(async {
        let directory = TempDir::new().expect("a directory");
        let path = directory.path().join("fitness.db");
        let store = SqlitePelotonClassStore::new(connect(&path).await.expect("a store opens"));

        let writer = another_writer(&path, Duration::from_millis(300));
        let listing = async {
            // Start once the other writer holds the lock.
            tokio::time::sleep(Duration::from_millis(100)).await;
            store.record_listed(&[listed("a-new-class")]).await
        };
        let (written, recorded) = tokio::join!(writer, listing);

        written.expect("the other writer commits");
        let added = recorded.expect("the listing waits for the lock rather than failing");
        assert_eq!(added, 1, "only the class it listed is new to it");
    });
}
