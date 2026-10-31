//! MyFitnessPal's export (#242): landed whole, and derived as the weights the
//! operator logged in it. The fixtures are small invented exports laid out as
//! the operator's is: a title row, then a header, then one row per thing
//! logged.
//!
//! Tests return `()` and assert by panicking. See `store.rs` for why.

use domain::normalised::StatedZones;
use std::{fs, path::Path};

use application::{
    Clock, StoreError, WeighInHistory as _, WorkoutExtractor as _, WorkoutNormaliser as _,
    canonicalise,
    extract::{Extraction, ExtractionPorts},
    normalise::{Normalisation, NormalisationPorts},
    ports::Translator as _,
};
use domain::{
    analytical::Weighed,
    canonical::Occurred,
    landing::FetchedAt,
    normalised::{OperatorZone, Refusal, RefusalReason},
};
use infrastructure::{
    FileRunLock, FolderFiles, MyFitnessPalExportAccountReader, MyFitnessPalExportLandingStore,
    MyFitnessPalWeighInTranslator, SqliteCanonicalWeighInStore, SqliteExtractionRunLog,
    SqliteMyFitnessPalStore, SqliteNormalisationRunLog, SqliteNormalisedWeighInReader, SqlitePool,
    SqliteRefusalStore, SqliteResumptionPointStore, connect,
};
use jiff::civil::date;

use crate::support::{layers, versions};

const FIRST: &[u8] = include_bytes!("fixtures/myfitnesspal/first-export.ods");
const SECOND: &[u8] = include_bytes!("fixtures/myfitnesspal/second-export.ods");

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

fn corrupt(error: impl std::fmt::Display) -> StoreError {
    StoreError::Corrupt {
        detail: error.to_string(),
    }
}

async fn land(folder: &Path, pool: &SqlitePool, database: &Path) -> Result<usize, StoreError> {
    let summary = Extraction::new(ExtractionPorts {
        source: FolderFiles::new(folder),
        landing: MyFitnessPalExportLandingStore::new(pool.clone()).map_err(corrupt)?,
        resumption: SqliteResumptionPointStore::new(pool.clone()),
        runs: SqliteExtractionRunLog::new(pool.clone()),
        lock: FileRunLock::beside(database),
        clock: FixedClock,
    })
    .extract()
    .await
    .map_err(corrupt)?;
    Ok(summary.records_landed.as_usize())
}

async fn normalise(pool: &SqlitePool) -> Result<usize, StoreError> {
    normalise_as(pool, application::Rederivation::Whole).await
}

async fn normalise_as(
    pool: &SqlitePool,
    rederivation: application::Rederivation,
) -> Result<usize, StoreError> {
    let stream = MyFitnessPalExportLandingStore::STREAM;
    let summary = Normalisation::new(
        NormalisationPorts {
            raw: MyFitnessPalExportAccountReader::new(pool.clone()).map_err(corrupt)?,
            translator: MyFitnessPalWeighInTranslator,
            workouts: SqliteMyFitnessPalStore::new(pool.clone()).map_err(corrupt)?,
            refusals: SqliteRefusalStore::new(pool.clone(), stream).map_err(corrupt)?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: FixedClock,
        },
        StatedZones::throughout(OperatorZone::try_from("Europe/London").map_err(corrupt)?),
    )
    .normalise(rederivation)
    .await
    .map_err(corrupt)?;
    Ok(summary.workouts_written.as_usize())
}

/// `on kg entered-at`, for every weigh-in held, in day order.
async fn held(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    Ok(sqlx::query_as::<_, (String, i64, String, String)>(
        "SELECT w.on_day, w.mass_grams, m.entered_at_utc, m.sheet || '!' || m.cell
         FROM weigh_in w JOIN myfitnesspal_weigh_in m ON m.weigh_in = w.id
         ORDER BY w.on_day",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(on, grams, entered, cell)| format!("{on} {grams}g {entered} {cell}"))
    .collect())
}

async fn refusals(pool: &SqlitePool) -> Result<Vec<String>, StoreError> {
    let stream = MyFitnessPalExportLandingStore::STREAM;
    let store = SqliteRefusalStore::new(pool.clone(), stream).map_err(corrupt)?;
    Ok(application::ports::RefusalStore::all(&store)
        .await?
        .iter()
        .map(|refusal: &Refusal| match &refusal.reason {
            RefusalReason::UnreadableValue { field, detail } => format!("{field}: {detail}"),
            other => format!("{other:?}"),
        })
        .collect())
}

fn folder_with(directory: &Path, files: &[(&str, &[u8])]) -> std::io::Result<std::path::PathBuf> {
    let folder = directory.join("myfitnesspal-exports");
    fs::create_dir_all(&folder)?;
    for (name, bytes) in files {
        fs::write(folder.join(name), bytes)?;
    }
    Ok(folder)
}

/// Every weight is one weigh-in, dated to the day it was logged against and
/// carrying when it was typed; a food is not a weigh-in, and a measurement of
/// anything else, or in another unit, is refused.
#[test]
fn every_weight_is_a_weigh_in_on_its_day() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let folder = folder_with(directory.path(), &[("export.ods", FIRST)]).expect("the folder");
        let database = directory.path().join("store.db");
        let pool = connect(&database).await.expect("a store");

        assert_eq!(land(&folder, &pool, &database).await.expect("lands"), 1);
        assert_eq!(land(&folder, &pool, &database).await.expect("again"), 0);
        assert_eq!(normalise(&pool).await.expect("derives"), 3);

        assert_eq!(
            held(&pool).await.expect("the weigh-ins"),
            [
                "2014-12-29 70100g 2015-01-13T11:31:25Z Sheet 1 - invented Data!F4",
                "2016-07-04 64200g 2016-07-04T06:23:04Z Sheet 1 - invented Data!F5",
                "2016-07-05 64000g 2016-07-05T06:17:28Z Sheet 1 - invented Data!F6",
            ]
        );

        let refused = refusals(&pool).await.expect("the refusals");
        assert_eq!(refused.len(), 2, "{refused:?}");
        assert!(
            refused[0].contains("Sheet 1 - invented Data!F7: Text(\"waist\")"),
            "{refused:?}"
        );
        assert!(
            refused[1].contains("Sheet 1 - invented Data!F8: Text(\"pounds\")"),
            "{refused:?}"
        );
    });
}

/// A second export is the same account sent again: a day it holds is read
/// from it, and a day only the first holds is still read from the first.
#[test]
fn the_export_landed_last_holds_each_day() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let folder = folder_with(directory.path(), &[("a.ods", FIRST)]).expect("the folder");
        let database = directory.path().join("store.db");
        let pool = connect(&database).await.expect("a store");
        land(&folder, &pool, &database).await.expect("the first");
        fs::write(folder.join("b.ods"), SECOND).expect("the second");
        assert_eq!(
            land(&folder, &pool, &database).await.expect("the second"),
            1
        );

        assert_eq!(normalise(&pool).await.expect("derives"), 4);
        let days: Vec<String> = held(&pool)
            .await
            .expect("the weigh-ins")
            .iter()
            .map(|line| line.split(' ').take(2).collect::<Vec<_>>().join(" "))
            .collect();
        assert_eq!(
            days,
            [
                "2014-12-29 70100g",
                "2016-07-04 64200g",
                "2016-07-05 64400g",
                "2016-07-08 63900g",
            ]
        );
    });
}

/// Relative strength reads MyFitnessPal's weights, dated to their day, and
/// nothing a rebuild of them wrote touches another source's rows.
#[test]
fn the_history_reads_a_weight_dated_to_a_day() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let folder = folder_with(directory.path(), &[("export.ods", FIRST)]).expect("the folder");
        let database = directory.path().join("store.db");
        let pool = connect(&database).await.expect("a store");
        land(&folder, &pool, &database).await.expect("lands");
        normalise(&pool).await.expect("derives");
        let canonical = SqliteCanonicalWeighInStore::new(pool.clone());
        let normalised = SqliteNormalisedWeighInReader::new(pool.clone());
        canonicalise::weigh_ins(&normalised, &canonical)
            .await
            .expect("canonicalises");
        normalise(&pool).await.expect("derives again");
        canonicalise::weigh_ins(&normalised, &canonical)
            .await
            .expect("canonicalises again");

        let history = canonical.weigh_ins().await.expect("the history");
        assert_eq!(history.len(), 3, "a rebuild replaces rather than adds");
        assert_eq!(
            history.first(),
            Some(&Weighed {
                at: Occurred::On(date(2014, 12, 29)),
                mass: domain::measure::Kg::from_grams(70_100),
                fat_free: None,
            })
        );
    });
}

/// A second export derived on its own run holds the rows a derivation of both
/// at once would (#571). Every export is one account, so the second reaches
/// all of it.
#[test]
fn an_incremental_derivation_writes_what_a_whole_one_would() {
    runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");

        let whole_folder =
            folder_with(&directory.path().join("whole"), &[("a.ods", FIRST)]).expect("a folder");
        let whole_database = directory.path().join("whole.db");
        let whole = connect(&whole_database).await.expect("a store");
        land(&whole_folder, &whole, &whole_database)
            .await
            .expect("the first");
        fs::write(whole_folder.join("b.ods"), SECOND).expect("the second");
        land(&whole_folder, &whole, &whole_database)
            .await
            .expect("the second");
        normalise_as(&whole, application::Rederivation::Whole)
            .await
            .expect("a whole derivation");

        let folder = folder_with(&directory.path().join("incremental"), &[("a.ods", FIRST)])
            .expect("a folder");
        let database = directory.path().join("incremental.db");
        let incremental = connect(&database).await.expect("a store");
        land(&folder, &incremental, &database)
            .await
            .expect("the first");
        normalise_as(&incremental, application::Rederivation::Incremental)
            .await
            .expect("a first derivation");
        fs::write(folder.join("b.ods"), SECOND).expect("the second");
        land(&folder, &incremental, &database)
            .await
            .expect("the second");
        normalise_as(&incremental, application::Rederivation::Incremental)
            .await
            .expect("an incremental derivation");

        assert_eq!(
            layers::dump(&incremental).await.expect("a dump"),
            layers::dump(&whole).await.expect("a dump"),
        );
    });
}

/// **Its version names what it translates** (#606): both exports, landed as
/// the operator lands them.
#[test]
fn its_version_names_what_it_translates() {
    let translations = runtime().expect("a runtime").block_on(async {
        let directory = tempfile::tempdir().expect("a directory");
        let folder = folder_with(
            directory.path(),
            &[("first.ods", FIRST), ("second.ods", SECOND)],
        )
        .expect("the folder");
        let database = directory.path().join("store.db");
        let pool = connect(&database).await.expect("a store");
        land(&folder, &pool, &database).await.expect("lands");

        let reader = MyFitnessPalExportAccountReader::new(pool.clone()).expect("a reader");
        let zones = StatedZones::throughout(OperatorZone::try_from("Europe/London").expect("zone"));
        versions::accounts(&reader)
            .await
            .expect("the accounts")
            .iter()
            .map(|account| {
                MyFitnessPalWeighInTranslator
                    .translate(account, &zones)
                    .expect("translated")
            })
            .collect::<Vec<_>>()
    });

    if let Err(unpinned) = versions::pinned::<MyFitnessPalWeighInTranslator>(&translations) {
        panic!("{unpinned}");
    }
}
