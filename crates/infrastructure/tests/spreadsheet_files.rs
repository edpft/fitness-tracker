//! Landing a folder of the operator's historical spreadsheets (#263).
//!
//! What is pinned is the shape agreed for them: one record per file holding its
//! bytes as they are, identity by the digest of those bytes, and the path and
//! modification time as provenance. So a byte-identical copy lands once, a
//! conflicted copy that differs lands separately, and landing the same folder
//! again lands nothing.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use std::{
    fmt::Write as _,
    fs,
    path::Path,
    time::{Duration, SystemTime},
};

use application::{
    Clock, ExtractionError, RunSummary, SourceError, StoreError, WorkoutExtractor as _,
    extract::{Extraction, ExtractionPorts},
};
use domain::landing::FetchedAt;
use infrastructure::{
    FileRunLock, SpreadsheetFileLandingStore, SpreadsheetFiles, SqliteExtractionRunLog, SqlitePool,
    SqliteResumptionPointStore, connect,
};
use sha2::{Digest as _, Sha256};
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

/// Not UTF-8, as a workbook is not: reading one as text would corrupt it.
const TRAINING: &[u8] = &[0x50, 0x4b, 0x03, 0x04, 0xff, 0xfe, 0x00, 0x80, 0x01];
const CONFLICTED: &[u8] = &[0x50, 0x4b, 0x03, 0x04, 0xff, 0xfe, 0x00, 0x80, 0x02];

/// 2017-03-14T09:26:53Z, as a filesystem states it.
const MODIFIED: Duration = Duration::from_secs(1_489_483_613);

/// A file with the given bytes, saved at [`MODIFIED`].
fn write(folder: &Path, relative: &str, bytes: &[u8]) -> std::io::Result<()> {
    let path = folder.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, bytes)?;
    fs::File::options()
        .write(true)
        .open(&path)?
        .set_modified(SystemTime::UNIX_EPOCH + MODIFIED)
}

/// Three files, and two of them the same bytes under different names.
fn folder_of_copies(folder: &Path) -> std::io::Result<()> {
    write(folder, "Dropbox/Random/CT 2017.xlsx", TRAINING)?;
    write(folder, "OneDrive/Documents/CT 2017.xlsx", TRAINING)?;
    write(
        folder,
        "Dropbox/Random/CT 2017 (Netbook's conflicted copy 2017-03-14).xlsx",
        CONFLICTED,
    )
}

/// The store a test lands into, and the lock file beside it.
struct Store {
    pool: SqlitePool,
    database: std::path::PathBuf,
}

async fn land(folder: &Path, store: &Store) -> Result<RunSummary, ExtractionError> {
    let Store { pool, database } = store;
    let landing =
        SpreadsheetFileLandingStore::new(pool.clone()).map_err(|error| StoreError::Corrupt {
            detail: error.to_string(),
        })?;
    Extraction::new(ExtractionPorts {
        source: SpreadsheetFiles::new(folder),
        landing,
        resumption: SqliteResumptionPointStore::new(pool.clone()),
        runs: SqliteExtractionRunLog::new(pool.clone()),
        lock: FileRunLock::beside(database),
        clock: FixedClock,
    })
    .extract()
    .await
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

type Row = (String, String, String, Vec<u8>);

async fn landed(pool: &SqlitePool) -> Result<Vec<Row>, sqlx::Error> {
    Ok(sqlx::query(
        "SELECT source_record_id, path, modified_at, payload
         FROM spreadsheet_file_landing ORDER BY id",
    )
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| {
        (
            row.get("source_record_id"),
            row.get("path"),
            row.get("modified_at"),
            row.get("payload"),
        )
    })
    .collect())
}

/// **Every file, once per distinct content, byte for byte.** The two copies of
/// the same workbook are one record; the conflicted copy that differs is its
/// own. The first path in order is the one a shared content is landed under.
#[test]
fn each_distinct_file_lands_once_with_its_path_and_modified_time() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let folder = directory.path().join("spreadsheets");
        folder_of_copies(&folder).expect("the files");
        let database = directory.path().join("store.db");
        let store = Store {
            pool: connect(&database).await.expect("a store"),
            database,
        };

        let summary = land(&folder, &store).await.expect("a run");

        assert_eq!(summary.events_seen.as_usize(), 3, "every file is read");
        assert_eq!(summary.records_landed.as_usize(), 2, "copies land once");
        assert_eq!(summary.resumption_point, None, "a folder has no feed");

        assert_eq!(
            landed(&store.pool).await.expect("the landed rows"),
            vec![
                (
                    hex(CONFLICTED),
                    "Dropbox/Random/CT 2017 (Netbook's conflicted copy 2017-03-14).xlsx".to_owned(),
                    "2017-03-14T09:26:53Z".to_owned(),
                    CONFLICTED.to_vec(),
                ),
                (
                    hex(TRAINING),
                    "Dropbox/Random/CT 2017.xlsx".to_owned(),
                    "2017-03-14T09:26:53Z".to_owned(),
                    TRAINING.to_vec(),
                ),
            ]
        );
    });
}

/// Landing the same files again lands nothing — the other half of the issue's
/// "done when".
#[test]
fn landing_the_same_folder_again_lands_nothing() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let folder = directory.path().join("spreadsheets");
        folder_of_copies(&folder).expect("the files");
        let database = directory.path().join("store.db");
        let store = Store {
            pool: connect(&database).await.expect("a store"),
            database,
        };

        land(&folder, &store).await.expect("the first run");
        let again = land(&folder, &store).await.expect("the second run");

        assert_eq!(again.events_seen.as_usize(), 3);
        assert_eq!(again.records_landed.as_usize(), 0);
    });
}

/// A file that has moved or been renamed is the same bytes, so it is the same
/// record; a file edited since is a new one.
#[test]
fn a_renamed_file_lands_nothing_and_an_edited_one_lands_again() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let folder = directory.path().join("spreadsheets");
        write(&folder, "Dropbox/Random/Weights.xlsx", TRAINING).expect("a file");
        let database = directory.path().join("store.db");
        let store = Store {
            pool: connect(&database).await.expect("a store"),
            database,
        };
        land(&folder, &store).await.expect("the first run");

        fs::rename(
            folder.join("Dropbox/Random/Weights.xlsx"),
            folder.join("Dropbox/Random/Weights (2016).xlsx"),
        )
        .expect("a rename");
        let renamed = land(&folder, &store).await.expect("the second run");
        assert_eq!(renamed.records_landed.as_usize(), 0);

        write(&folder, "Dropbox/Random/Weights (2016).xlsx", CONFLICTED).expect("an edit");
        let edited = land(&folder, &store).await.expect("the third run");
        assert_eq!(edited.records_landed.as_usize(), 1);
    });
}

/// **A folder that is not there fails the run**, rather than landing nothing
/// and reporting success: an empty result would read as "nothing new".
#[test]
fn a_missing_folder_fails_visibly() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let database = directory.path().join("store.db");
        let store = Store {
            pool: connect(&database).await.expect("a store"),
            database,
        };

        let refused = land(&directory.path().join("nowhere"), &store).await;

        assert!(
            matches!(
                refused,
                Err(ExtractionError::Source(SourceError::Unavailable { .. }))
            ),
            "{refused:?}"
        );
    });
}

/// **An empty file is refused by name**, because a payload is never empty and
/// the operator put the folder together: the fix is theirs to make, so the
/// message says which file.
#[test]
fn an_empty_file_is_refused_by_name() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let folder = directory.path().join("spreadsheets");
        write(&folder, "OneDrive/Book (9).xlsx", b"").expect("an empty file");
        let database = directory.path().join("store.db");
        let store = Store {
            pool: connect(&database).await.expect("a store"),
            database,
        };

        let refused = land(&folder, &store).await;

        let Err(ExtractionError::Source(SourceError::Malformed { detail })) = refused else {
            panic!("an empty file is refused, got {refused:?}");
        };
        assert!(detail.contains("OneDrive/Book (9).xlsx"), "{detail}");
    });
}
