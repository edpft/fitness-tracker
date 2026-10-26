//! Deriving only what landed since the last derivation, over the Hevy corpus
//! and the real store (#571).
//!
//! The corpus is landed in two halves, derived after each, and compared row for
//! row with a store that landed it whole and derived once. The corpus holds
//! sessions split across routines and workouts served more than once, so the
//! second half reaches sessions the first half began.

mod support;

use application::{
    ExtractionRunLog as _, LandingStore as _, NormalisationSummary, Rederivation,
    WorkoutNormaliser as _,
    normalise::{Normalisation, NormalisationPorts},
};
use domain::normalised::StatedZones;
use infrastructure::{
    HevySessionAccountReader, HevySessionTranslator, HevyWorkoutLandingStore,
    SqliteExtractionRunLog, SqliteGymSessionStore, SqliteNormalisationRunLog, SqliteRefusalStore,
    connect,
};
use sqlx::SqlitePool;

use support::{corpus, layers};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

async fn land(pool: &SqlitePool, records: &[domain::landing::LandedRecord]) -> Fallible<()> {
    let landing = HevyWorkoutLandingStore::new(pool.clone())?;
    let runs = SqliteExtractionRunLog::new(pool.clone());
    let run = runs
        .begin(landing.stream(), domain::landing::FetchedAt::EPOCH)
        .await?;
    landing
        .append(
            run,
            records
                .iter()
                .map(|landed| landed.record().clone())
                .collect(),
        )
        .await?;
    Ok(())
}

async fn derive(pool: &SqlitePool, rederivation: Rederivation) -> Fallible<NormalisationSummary> {
    Ok(Normalisation::new(
        NormalisationPorts {
            raw: HevySessionAccountReader::new(pool.clone())?,
            translator: HevySessionTranslator::default(),
            workouts: SqliteGymSessionStore::new(pool.clone())?,
            refusals: SqliteRefusalStore::new(pool.clone(), HevyWorkoutLandingStore::STREAM)?,
            runs: SqliteNormalisationRunLog::new(pool.clone()),
            clock: corpus::FixedClock,
        },
        StatedZones::throughout(corpus::zone()?),
    )
    .normalise(rederivation)
    .await?)
}

fn runtime() -> Fallible<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?)
}

#[test]
fn an_incremental_derivation_writes_what_a_whole_one_would() {
    let runtime = runtime().expect("a runtime");
    runtime.block_on(async {
        let records = corpus::records().expect("the corpus");
        let (first, second) = records.split_at(records.len() / 2);
        let directory = tempfile::tempdir().expect("a directory");

        let whole = connect(&directory.path().join("whole.db"))
            .await
            .expect("a store");
        land(&whole, first).await.expect("the first half");
        land(&whole, second).await.expect("the second half");
        let all = derive(&whole, Rederivation::Whole)
            .await
            .expect("a whole derivation");

        let incremental = connect(&directory.path().join("incremental.db"))
            .await
            .expect("a store");
        land(&incremental, first).await.expect("the first half");
        derive(&incremental, Rederivation::Incremental)
            .await
            .expect("a first derivation");
        land(&incremental, second).await.expect("the second half");
        let since = derive(&incremental, Rederivation::Incremental)
            .await
            .expect("an incremental derivation");

        assert_eq!(
            layers::dump(&incremental).await.expect("a dump"),
            layers::dump(&whole).await.expect("a dump"),
        );
        assert!(since.reconciles(), "every record read has one outcome");
        assert!(
            since.records_read.as_usize() < all.records_read.as_usize(),
            "read {} of {}",
            since.records_read.as_usize(),
            all.records_read.as_usize()
        );
        assert_eq!(since.records_covered, all.records_covered);

        // Nothing landed since: nothing is read and nothing written.
        let again = derive(&incremental, Rederivation::Incremental)
            .await
            .expect("a derivation with nothing new");
        assert_eq!(again.records_read.as_usize(), 0);
        assert_eq!(again.workouts_written.as_usize(), 0);
        assert_eq!(
            layers::dump(&incremental).await.expect("a dump"),
            layers::dump(&whole).await.expect("a dump"),
        );
    });
}
