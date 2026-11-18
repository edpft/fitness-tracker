//! What the store answered is kept until this process next writes it (#830).
//!
//! The operator, 2026-11-18: *"the fix is that the CLI and web server use the
//! same backend, until they do it's okay for the web server to be unaware of
//! changes made by the CLI."*
#![expect(
    clippy::disallowed_methods,
    reason = "a second pool stands for another process writing the store"
)]

use configurator::{catalogue, corrections, wiring};
use domain::{
    gym::exercise::{Exercise, RepsExercise},
    landing::SourceRecordId,
    normalised::{CorrectedTerm, CorrectionReason, OperatorZone, SourceTerm},
    sequence::NonEmpty,
};
use infrastructure::{HevyWorkoutLandingStore, SqliteEditOverlayStore, connect};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

fn a_term(id: &str) -> Fallible<NonEmpty<CorrectedTerm>> {
    Ok(NonEmpty::new(vec![CorrectedTerm::new(
        SourceRecordId::try_from(id.to_owned())?,
        SourceTerm::try_from("1B2B1E7C")?,
    )])?)
}

/// What Hevy's layer is derived from: as first read, after another process
/// asserts a correction, after this one does, and from a store opened after.
async fn around_two_writers(database: &std::path::Path) -> Fallible<[application::DerivedFrom; 4]> {
    let zone = OperatorZone::try_from("Europe/London".to_owned())?;
    let hevy = catalogue::lookup("hevy.workouts").ok_or("hevy is catalogued")?;
    let store = configurator::store::Store::open(database).await?;
    let reason = CorrectionReason::try_from("Hevy had no neutral grip pull up")?;

    let first = wiring::inputs(hevy, &store, &zone).await?;

    let elsewhere_term = a_term("from-another-process")?;
    let here_term = a_term("from-this-process")?;
    SqliteEditOverlayStore::new(connect(database).await?, HevyWorkoutLandingStore::STREAM)?
        .assert(
            Exercise::Reps(RepsExercise::NeutralGripPullUp),
            "2026-10-18T09:00:00Z".parse()?,
            &reason,
            &elsewhere_term,
        )
        .await?;
    let elsewhere = wiring::inputs(hevy, &store, &zone).await?;

    corrections::assert_exercise(
        &store,
        Exercise::Reps(RepsExercise::NeutralGripPullUp),
        &reason,
        &here_term,
    )
    .await?;
    let here = wiring::inputs(hevy, &store, &zone).await?;

    let reopened = wiring::inputs(
        hevy,
        &configurator::store::Store::open(database).await?,
        &zone,
    )
    .await?;
    Ok([first, elsewhere, here, reopened])
}

/// **A write by another process is not seen until this one writes**, and
/// then everything it wrote is.
#[test]
fn a_read_is_kept_until_this_process_writes() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let [first, elsewhere, here, reopened] = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(around_two_writers(&directory.path().join("test.db")))
        .expect("the store answers");

    assert_eq!(first, elsewhere, "another process's write is not seen");
    assert_ne!(elsewhere, here, "this process's write is");
    assert_eq!(here, reopened, "and reads as a store opened afresh does");
}
