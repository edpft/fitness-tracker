//! What each derived layer was derived from, through the real store (#468).
//!
//! The use case's suite decides what to derive over a graph in memory. This one
//! asserts the half it cannot: that the store hands back what was recorded, and
//! that what a canonical layer is derived from moves exactly when a
//! normalisation rewrites the rows it reads.
//!
//! Tests return `()` and assert by panicking; the helpers return `Result` for
//! the test to unwrap.

use crate::support;

use application::{
    CanonicalLayer, DerivationLedger as _, DerivedFrom, NormalisationRunLog as _,
    NormalisedGymSessionReader as _, NormalisedWeighInReader as _, WorkoutNormaliser as _,
    normalise::{Normalisation, NormalisationPorts},
};
use domain::normalised::StatedZones;
use domain::{
    landing::{FetchedAt, LandingStream},
    normalised::{NormalisationOutcome, NormalisationRunId},
};
use infrastructure::{
    HevySessionAccountReader, HevySessionTranslator, HevyWorkoutLandingStore,
    SqliteDerivationLedger, SqliteGymSessionStore, SqliteNormalisationRunLog,
    SqliteNormalisedGymSessionReader, SqliteNormalisedWeighInReader, SqliteRefusalStore, connect,
};
use sqlx::SqlitePool;
use support::{corpus, store};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// Derive Hevy's normalised layer again from what raw holds.
async fn normalise_hevy(pool: &SqlitePool) -> Fallible<NormalisationRunId> {
    let summary = Normalisation::new(
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
    .normalise(application::Rederivation::Whole)
    .await?;
    Ok(summary.run_id)
}

/// **The gym's inputs move when, and only when, a normalisation rewrites its
/// rows**: read twice they agree, and a second derivation of Hevy changes them.
#[test]
fn a_canonical_layers_inputs_move_with_the_derivation_that_wrote_them() {
    let read = corpus::block_on(async {
        let (_directory, pool) = store::derived_and_authored().await?;
        let reader = SqliteNormalisedGymSessionReader::new(pool.clone());
        let first = reader.written_by().await?;
        let again = reader.written_by().await?;
        let run = normalise_hevy(&pool).await?;
        let after = reader.written_by().await?;
        let weigh_ins = SqliteNormalisedWeighInReader::new(pool)
            .written_by()
            .await?;
        Ok::<_, Box<dyn std::error::Error>>((first, again, run, after, weigh_ins))
    });
    let Ok(Ok((first, again, run, after, weigh_ins))) = read else {
        panic!("the store reads")
    };

    assert_eq!(first, again);
    assert_ne!(first, after);
    // The run, and how many of its rows stand (#571).
    assert!(
        after
            .as_str()
            .starts_with(&format!("hevy.workouts {} (", run.as_u64())),
        "{after:?}"
    );
    assert_eq!(weigh_ins, DerivedFrom::from(String::new()));
}

/// **The ledger hands back what was recorded**, and a normalised layer reads
/// as derived from what its latest success recorded: one derived since without
/// recording, as one started by hand is, reads as derived from nothing known.
#[test]
fn the_ledger_hands_back_what_each_layer_was_derived_from() {
    let read = corpus::block_on(async {
        let directory = tempfile::tempdir()?;
        let pool = connect(&directory.path().join("test.db")).await?;
        let ledger = SqliteDerivationLedger::new(pool.clone());
        let runs = SqliteNormalisationRunLog::new(pool);
        let stream = LandingStream::try_from(HevyWorkoutLandingStore::STREAM)?;
        let succeed = |run| {
            runs.finish(
                run,
                NormalisationOutcome::Succeeded {
                    finished_at: FetchedAt::EPOCH,
                    records_read: 0.into(),
                    workouts_written: 0.into(),
                    records_composed: 0.into(),
                    records_superseded: 0.into(),
                    workouts_retracted: 0.into(),
                    retractions_read: 0.into(),
                    records_refused: 0.into(),
                    refusals_recorded: 0.into(),
                    records_covered: 0.into(),
                },
            )
        };
        let from = DerivedFrom::from("records 164; corrections ; zone Europe/London".to_owned());

        let no_canonical = ledger.canonical(CanonicalLayer::Gym).await?;
        ledger.record_canonical(CanonicalLayer::Gym, &from).await?;
        let canonical = ledger.canonical(CanonicalLayer::Gym).await?;
        let cycling = ledger.canonical(CanonicalLayer::Cycling).await?;

        let run = runs.begin(&stream, FetchedAt::EPOCH).await?;
        succeed(run).await?;
        ledger.record_normalised(run, &from).await?;
        let normalised = ledger.normalised(&stream).await?;
        let by_hand = runs.begin(&stream, FetchedAt::EPOCH).await?;
        succeed(by_hand).await?;
        let since = ledger.normalised(&stream).await?;

        Ok::<_, Box<dyn std::error::Error>>((
            no_canonical,
            canonical,
            cycling,
            normalised,
            since,
            from,
        ))
    });
    let Ok(Ok((no_canonical, canonical, cycling, normalised, since, from))) = read else {
        panic!("the store reads")
    };

    assert_eq!(no_canonical, None);
    assert_eq!(canonical, Some(from.clone()));
    assert_eq!(cycling, None);
    assert_eq!(normalised, Some(from));
    assert_eq!(since, None);
}

/// **Whatever empties a canonical layer forgets what it was derived from**
/// (#606). A spreadsheet revised for nothing gym-shaped still empties the gym
/// layer, and leaves every gym session's run and count as they were, so the
/// layer's inputs alone would read as current and it would stay empty.
#[test]
fn emptying_a_canonical_layer_forgets_its_derivation() {
    use application::ports::NormalisedEntityStore as _;
    use infrastructure::{SpreadsheetFileLandingStore, SqliteSpreadsheetStore};

    let read = corpus::block_on(async {
        let directory = tempfile::tempdir()?;
        let pool = connect(&directory.path().join("test.db")).await?;
        let ledger = SqliteDerivationLedger::new(pool.clone());
        let from = DerivedFrom::from("version 1; hevy.workouts 1 (151)".to_owned());
        ledger.record_canonical(CanonicalLayer::Gym, &from).await?;
        ledger
            .record_canonical(CanonicalLayer::Cycling, &from)
            .await?;
        let before = ledger.canonical(CanonicalLayer::Gym).await?;

        let stream = LandingStream::try_from(SpreadsheetFileLandingStore::STREAM)?;
        let run = SqliteNormalisationRunLog::new(pool.clone())
            .begin(&stream, FetchedAt::EPOCH)
            .await?;
        SqliteSpreadsheetStore::new(pool.clone())?
            .revise(run, Vec::new(), Vec::new())
            .await?;

        let gym = ledger.canonical(CanonicalLayer::Gym).await?;
        let cycling = ledger.canonical(CanonicalLayer::Cycling).await?;
        Ok::<_, Box<dyn std::error::Error>>((before, gym, cycling, from))
    });
    let Ok(Ok((before, gym, cycling, from))) = read else {
        panic!("the store reads")
    };

    assert_eq!(before, Some(from.clone()));
    assert_eq!(
        gym, None,
        "the gym layer was emptied, so it is derived again"
    );
    assert_eq!(cycling, Some(from), "the cycling layer was not touched");
}
