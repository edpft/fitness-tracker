//! What a normalised layer is derived from, as the composition root states it
//! (#468).
//!
//! **A correction is one of the layer's inputs**, so asserting one moves them
//! and retracting it moves them back: the layer derived under the correction
//! is then not the one the inputs describe, and is derived again.

use configurator::{catalogue, wiring};
use domain::{
    gym::exercise::{Exercise, RepsExercise},
    landing::SourceRecordId,
    normalised::{CorrectedTerm, CorrectionReason, OperatorZone, SourceTerm},
    sequence::NonEmpty,
};
use infrastructure::{HevyWorkoutLandingStore, SqliteEditOverlayStore, connect};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// The inputs of `stream` before a correction to Hevy, with it, and after it
/// is retracted, and those of Peloton's layer with it.
async fn around_a_correction(
    database: &std::path::Path,
) -> Fallible<[application::DerivedFrom; 5]> {
    let zone = OperatorZone::try_from("Europe/London".to_owned())?;
    let hevy = catalogue::lookup("hevy.workouts").ok_or("hevy is catalogued")?;
    let peloton = catalogue::lookup("peloton.rides").ok_or("peloton is catalogued")?;
    let inputs = async |known| wiring::inputs(known, database, &zone).await;

    let before = inputs(hevy).await?;
    let peloton_before = inputs(peloton).await?;

    let overlay =
        SqliteEditOverlayStore::new(connect(database).await?, HevyWorkoutLandingStore::STREAM)?;
    let term = CorrectedTerm::new(
        SourceRecordId::try_from("a-workout")?,
        SourceTerm::try_from("1B2B1E7C")?,
    );
    let id = overlay
        .assert(
            Exercise::Reps(RepsExercise::NeutralGripPullUp),
            "2026-10-18T09:00:00Z".parse()?,
            &CorrectionReason::try_from("Hevy had no neutral grip pull up")?,
            &NonEmpty::new(vec![term])?,
        )
        .await?;
    let corrected = inputs(hevy).await?;
    let peloton_corrected = inputs(peloton).await?;
    overlay.retract(id).await?;
    let retracted = inputs(hevy).await?;

    Ok([
        before,
        corrected,
        retracted,
        peloton_before,
        peloton_corrected,
    ])
}

#[test]
fn a_correction_moves_the_inputs_of_the_layer_it_corrects_and_no_other() {
    let directory = tempfile::tempdir().expect("a scratch directory");
    let database = directory.path().join("test.db");
    let [
        before,
        corrected,
        retracted,
        peloton_before,
        peloton_corrected,
    ] = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime")
        .block_on(around_a_correction(&database))
        .expect("the store answers");

    assert_ne!(before, corrected);
    assert_eq!(before, retracted);
    assert_eq!(peloton_before, peloton_corrected);
}
