//! Landing the Wahoo app's recordings (#421).
//!
//! The folder itself is pinned in `spreadsheet_files.rs`, which reads one the
//! same way. What is pinned here is the table: every file lands whole, in its
//! own table, whatever it holds, and landing them again lands nothing.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::{fs, path::Path};

use application::{
    Clock, ExtractionError, RunSummary, StoreError, WorkoutExtractor as _,
    extract::{Extraction, ExtractionPorts},
};
use domain::landing::FetchedAt;
use infrastructure::{
    FileRunLock, FolderFiles, SqliteExtractionRunLog, SqlitePool, SqliteResumptionPointStore,
    WahooFileLandingStore, connect,
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

/// A FIT file's 14-byte header: protocol 2.0, profile 21.60, ten data bytes,
/// `.FIT`, and a header checksum of zero, which the format allows.
const HEADER: [u8; 14] = [
    0x0e, 0x20, 0x70, 0x08, 0x0a, 0x00, 0x00, 0x00, b'.', b'F', b'I', b'T', 0x00, 0x00,
];

/// Two recordings, as the app names them: a training session and a ride.
/// Their bytes differ, which is all that identifies them.
fn recordings() -> [(&'static str, Vec<u8>); 2] {
    let file = |body: &[u8]| [HEADER.as_slice(), body].concat();
    [
        (
            "2024-09-09-180326-WAHOOAPPDROID3A41-7-0.fit",
            file(&[
                0x40, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x02, 0x84, 0x0a, 0x00,
            ]),
        ),
        (
            "2024-05-17-181120-WAHOOAPPDROID3A41-6-0.fit",
            file(&[
                0x40, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x02, 0x84, 0x02, 0x00,
            ]),
        ),
    ]
}

async fn land(
    folder: &Path,
    pool: &SqlitePool,
    database: &Path,
) -> Result<RunSummary, ExtractionError> {
    let landing =
        WahooFileLandingStore::new(pool.clone()).map_err(|error| StoreError::Corrupt {
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

/// Every file lands byte for byte under its own name; a second run lands
/// nothing.
#[test]
fn every_file_lands_whole_once() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let folder = directory.path().join("WahooFitness");
        fs::create_dir_all(&folder).expect("the folder");
        for (name, bytes) in recordings() {
            fs::write(folder.join(name), bytes).expect("a recording");
        }
        let database = directory.path().join("store.db");
        let pool = connect(&database).await.expect("a store");

        let first = land(&folder, &pool, &database)
            .await
            .expect("the first run");
        assert_eq!(first.records_landed.as_usize(), 2);

        let landed: Vec<(String, Vec<u8>)> =
            sqlx::query("SELECT path, payload FROM wahoo_file_landing ORDER BY path")
                .fetch_all(&pool)
                .await
                .expect("the rows")
                .iter()
                .map(|row| (row.get("path"), row.get("payload")))
                .collect();
        let mut expected: Vec<(String, Vec<u8>)> = recordings()
            .into_iter()
            .map(|(name, bytes)| (name.to_owned(), bytes))
            .collect();
        expected.sort();
        assert_eq!(landed, expected);

        let again = land(&folder, &pool, &database)
            .await
            .expect("the second run");
        assert_eq!(again.events_seen.as_usize(), 2);
        assert_eq!(again.records_landed.as_usize(), 0);
    });
}
