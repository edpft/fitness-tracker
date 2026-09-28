//! Landing Beyond The White Board's export (#265).
//!
//! The folder itself is pinned in `spreadsheet_files.rs`, which reads one the
//! same way. What is pinned here is the table: the export lands whole, in its
//! own table rather than the spreadsheets', and landing it again lands nothing.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::{fs, path::Path};

use application::{
    Clock, ExtractionError, RunSummary, StoreError, WorkoutExtractor as _,
    extract::{Extraction, ExtractionPorts},
};
use domain::landing::FetchedAt;
use infrastructure::{
    BtwbExportLandingStore, FileRunLock, FolderFiles, SqliteExtractionRunLog, SqlitePool,
    SqliteResumptionPointStore, connect,
};
use sqlx::Row as _;

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> FetchedAt {
        FetchedAt::EPOCH
    }
}

/// The export's header and its first result, as BTWB sent them.
const EXPORT: &str = "Date,Formatted Result,Result,Performed,Workout,Description,Notes
2024-08-23,\"1950 kg | 90 kg, 100 kg, 100 kg, and 100 kg\",1950.0,prescribed,Deadlift : 5-5-5-5,\"Sets
5 Deadlifts | 90 kg
5 Deadlifts | 100 kg
5 Deadlifts | 100 kg
5 Deadlifts | 100 kg\",\"\"
";

const FILE: &str = "2026-09-28-workout_sessions.csv";

async fn land(
    folder: &Path,
    pool: &SqlitePool,
    database: &Path,
) -> Result<RunSummary, ExtractionError> {
    let landing =
        BtwbExportLandingStore::new(pool.clone()).map_err(|error| StoreError::Corrupt {
            detail: error.to_string(),
        })?;
    Extraction::new(ExtractionPorts {
        source: FolderFiles::new(folder),
        landing,
        resumption: SqliteResumptionPointStore::new(pool.clone()),
        runs: SqliteExtractionRunLog::new(pool.clone()),
        lock: FileRunLock::beside(database),
        clock: FixedClock,
    })
    .extract()
    .await
}

async fn rows(pool: &SqlitePool, sql: &'static str) -> Result<Vec<(String, Vec<u8>)>, sqlx::Error> {
    Ok(sqlx::query(sql)
        .fetch_all(pool)
        .await?
        .iter()
        .map(|row| (row.get("path"), row.get("payload")))
        .collect())
}

/// The export lands byte for byte under its own name, and only in its own
/// table; a second run lands nothing.
#[test]
fn the_export_lands_whole_once() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let folder = directory.path().join("btwb");
        fs::create_dir_all(&folder).expect("the folder");
        fs::write(folder.join(FILE), EXPORT).expect("the export");
        let database = directory.path().join("store.db");
        let pool = connect(&database).await.expect("a store");

        let first = land(&folder, &pool, &database)
            .await
            .expect("the first run");
        assert_eq!(first.records_landed.as_usize(), 1);

        assert_eq!(
            rows(
                &pool,
                "SELECT path, payload FROM btwb_export_landing ORDER BY id"
            )
            .await
            .expect("the rows"),
            vec![(FILE.to_owned(), EXPORT.as_bytes().to_vec())]
        );
        assert!(
            rows(&pool, "SELECT path, payload FROM spreadsheet_file_landing")
                .await
                .expect("the spreadsheets' rows")
                .is_empty(),
            "a separate source is a separate table"
        );

        let again = land(&folder, &pool, &database)
            .await
            .expect("the second run");
        assert_eq!(again.events_seen.as_usize(), 1);
        assert_eq!(again.records_landed.as_usize(), 0);
    });
}
