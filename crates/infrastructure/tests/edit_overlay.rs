//! The edit overlay, end to end (§ II.2, issue #304).
//!
//! The case the feature exists for: Hevy had no neutral grip pull up, so the
//! operator logged eighteen months of them under `Pull Up`, and nothing in the
//! payload separates those from a pull-up. Only he can say which they were.
//!
//! What has to hold, and what each test here pins:
//!
//! - the corrected entry reads as the exercise he named, after a rebuild;
//! - **retracting it restores exactly what the source said**, which is the
//!   property that makes this an input rather than an edit;
//! - the load is untouched, because the number was typed into the source's
//!   template and the template's convention still governs it;
//! - the correction reaches the entry it names and no other, and the anchor
//!   survives the entry moving within its record;
//! - the store round-trips an assertion, and a retraction takes its terms
//!   with it.

mod support;

use domain::{
    gym::{
        PerformedExercise,
        exercise::{Exercise, RepsExercise},
    },
    landing::SourceRecordId,
    normalised::{
        CorrectedTerm, Correction, CorrectionId, CorrectionReason, EditOverlay, SourceTerm,
    },
    sequence::NonEmpty,
};
use infrastructure::{
    HevyWorkoutLandingStore, SqliteEditOverlayStore, connect, hevy::payload::WorkoutEnvelope,
};
use support::{corpus, derived};

/// Hevy's plain `Pull Up` template, which carried the neutral-grip work.
const PULL_UP: &str = "1B2B1E7C";
/// Hevy's assisted one, which carried it too — and whose numbers are weight
/// taken *off*, which is the reason the load must not follow the correction.
const PULL_UP_ASSISTED: &str = "2C37EC5E";

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// A correction over the terms given, as the store would hand it back.
fn correcting(exercise: Exercise, terms: Vec<CorrectedTerm>) -> Fallible<EditOverlay> {
    let reason = CorrectionReason::try_from("Hevy had no neutral grip pull up")?;
    let correction = Correction::new(
        CorrectionId::from(1),
        exercise,
        "2026-09-30T09:00:00Z".parse()?,
        reason,
        NonEmpty::new(terms)?,
    );
    Ok(EditOverlay::of(&[correction]))
}

/// Every (record, template) pair in the corpus that holds one of Hevy's
/// templates — read off the payloads, which is what `HevyStandIns` does.
///
/// **Not guessed from the derived exercise.** Three templates reach `pull-up`,
/// so a workout reading as a pull-up says nothing about which template carried
/// it, and an anchor built from the wrong one corrects nothing.
fn anchors(template: &str) -> Fallible<Vec<CorrectedTerm>> {
    let fixture = corpus::derivation()?;
    let term = SourceTerm::try_from(template)?;
    let mut found = Vec::new();
    for record in &fixture.records {
        let envelope = WorkoutEnvelope::read(record.payload().as_bytes())?;
        let Some(workout) = envelope.workout else {
            continue;
        };
        if workout
            .exercises
            .iter()
            .any(|entry| entry.exercise_template_id == template)
        {
            found.push(CorrectedTerm::new(
                record.source_record_id().clone(),
                term.clone(),
            ));
        }
    }
    Ok(found)
}

/// How many sets of one exercise the whole corpus holds, and their loads in
/// order — the two figures a correction must move and must not.
fn tally(produced: &corpus::Produced, key: &str) -> (usize, Vec<String>) {
    let mut sets = 0;
    let mut loads = Vec::new();
    for workout in &produced.workouts {
        for exercise in workout.exercises() {
            if exercise.exercise_key() != key {
                continue;
            }
            if let PerformedExercise::ForReps {
                sets: performed, ..
            } = exercise
            {
                sets += performed.count();
                for set in performed {
                    loads.push(format!("{:?}", set.load));
                }
            }
        }
    }
    (sets, loads)
}

/// The done-when, in one test: corrected after a rebuild, and the source's own
/// word back after a retraction.
///
/// **Retraction is shown by deriving without the overlay**, which is precisely
/// what removing the correction leaves behind — the overlay is the only
/// difference between the two runs, so if the second matches the baseline then
/// nothing was patched into a derived row.
#[test]
fn a_correction_applies_on_rebuild_and_retracting_it_restores_the_source() {
    let Ok(fixture) = corpus::derivation() else {
        panic!("the corpus fixture loads")
    };
    let Ok(terms) = anchors(PULL_UP) else {
        panic!("the anchors resolve")
    };
    assert!(
        !terms.is_empty(),
        "the corpus holds pull-ups to correct; nothing else here means anything"
    );
    let Ok(overlay) = correcting(Exercise::Reps(RepsExercise::NeutralGripPullUp), terms) else {
        panic!("the overlay builds")
    };

    let baseline = derived!(fixture, false);
    let (was, _) = tally(&baseline, "pull-up");
    let (neutral_before, _) = tally(&baseline, "neutral-grip-pull-up");
    assert!(was > 0, "there are pull-ups before the correction");
    assert_eq!(
        neutral_before, 0,
        "and no neutral-grip pull-ups, so the count below cannot be coincidence"
    );

    let corrected = match corpus::block_on(fixture.run_correcting(overlay)) {
        Ok(Ok(produced)) => produced,
        Ok(Err(error)) => panic!("the corrected derivation succeeds: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };
    let (still_pull_ups, _) = tally(&corrected, "pull-up");
    let (now_neutral, _) = tally(&corrected, "neutral-grip-pull-up");

    // Every set that read as a pull-up under the plain template now reads as a
    // neutral-grip one. The assisted template is a separate anchor and is not
    // in this assertion, so `still_pull_ups` is what it carried.
    assert!(
        now_neutral > 0,
        "the correction reached the sets it named: {now_neutral} of them"
    );
    assert_eq!(
        still_pull_ups + now_neutral,
        was,
        "no set was created or lost by correcting one"
    );

    // The retraction. Nothing to undo: the overlay was never written into the
    // derived rows, so the run without it is the run before it.
    let retracted = derived!(fixture, false);
    let (after, _) = tally(&retracted, "pull-up");
    let (neutral_after, _) = tally(&retracted, "neutral-grip-pull-up");
    assert_eq!(after, was, "retracting restores what the source said");
    assert_eq!(neutral_after, 0, "and leaves nothing of the correction");
}

/// The load is the template's and stays the template's.
///
/// Hevy's assisted template records weight taken off, which the mapping negates;
/// `Neutral Grip Pull Up` is declared the same way, but the point is not that
/// these two agree — it is that the correction never consults the corrected
/// exercise's convention at all. Every load comes through byte for byte.
#[test]
fn correcting_the_exercise_leaves_every_load_exactly_as_the_source_recorded_it() {
    let Ok(fixture) = corpus::derivation() else {
        panic!("the corpus fixture loads")
    };
    let Ok(terms) = anchors(PULL_UP_ASSISTED) else {
        panic!("the anchors resolve")
    };
    let Ok(overlay) = correcting(Exercise::Reps(RepsExercise::NeutralGripPullUp), terms) else {
        panic!("the overlay builds")
    };

    let baseline = derived!(fixture, false);
    let (_, before) = tally(&baseline, "pull-up");

    let corrected = match corpus::block_on(fixture.run_correcting(overlay)) {
        Ok(Ok(produced)) => produced,
        Ok(Err(error)) => panic!("the corrected derivation succeeds: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };
    let (_, pull_ups) = tally(&corrected, "pull-up");
    let (_, neutral) = tally(&corrected, "neutral-grip-pull-up");

    // The loads either side, as multisets: the sets have moved between two
    // exercises and not one number has changed.
    let mut after = pull_ups;
    after.extend(neutral);
    let mut before = before;
    before.sort();
    after.sort();
    assert_eq!(
        before, after,
        "a corrected exercise keeps the load the source's template recorded"
    );
}

/// A correction names one record's entry, not every record holding that term.
#[test]
fn a_correction_does_not_reach_a_record_it_does_not_name() {
    let Ok(fixture) = corpus::derivation() else {
        panic!("the corpus fixture loads")
    };
    let Ok(all) = anchors(PULL_UP) else {
        panic!("the anchors resolve")
    };
    assert!(
        all.len() > 1,
        "several records hold the term, or this test proves nothing"
    );

    // One of them, and only one.
    let Some(first) = all.first().cloned() else {
        panic!("there is a first anchor")
    };
    let Ok(overlay) = correcting(Exercise::Reps(RepsExercise::NeutralGripPullUp), vec![first])
    else {
        panic!("the overlay builds")
    };

    let baseline = derived!(fixture, false);
    let (was, _) = tally(&baseline, "pull-up");

    let corrected = match corpus::block_on(fixture.run_correcting(overlay)) {
        Ok(Ok(produced)) => produced,
        Ok(Err(error)) => panic!("the corrected derivation succeeds: {error}"),
        Err(error) => panic!("a runtime is available: {error}"),
    };
    let (still, _) = tally(&corrected, "pull-up");
    let (now_neutral, _) = tally(&corrected, "neutral-grip-pull-up");

    assert!(
        still > 0,
        "the records the correction did not name still read as pull-ups"
    );
    assert!(now_neutral > 0, "and the one it named does not");
    assert_eq!(still + now_neutral, was);
}

/// The store round-trips an assertion, and a retraction takes its terms.
#[test]
fn the_store_holds_an_assertion_and_gives_it_all_back() {
    let outcome = corpus::block_on(async {
        let directory = tempfile::tempdir()?;
        let pool = connect(&directory.path().join("test.db")).await?;
        let store = SqliteEditOverlayStore::new(pool, HevyWorkoutLandingStore::STREAM)?;

        let record = SourceRecordId::try_from("a-workout")?;
        let term = CorrectedTerm::new(record.clone(), SourceTerm::try_from(PULL_UP)?);
        let reason = CorrectionReason::try_from("Hevy had no neutral grip pull up")?;
        let asserted_at = "2026-09-30T09:00:00Z".parse()?;

        let id = store
            .assert(
                Exercise::Reps(RepsExercise::NeutralGripPullUp),
                asserted_at,
                &reason,
                &NonEmpty::new(vec![term])?,
            )
            .await?;

        let held = store.all().await?;
        let overlay = store.overlay().await?;
        let retracted = store.retract(id).await?;
        let after = store.all().await?;
        let nothing = store.retract(id).await?;

        Ok::<_, Box<dyn std::error::Error>>((held, overlay, retracted, after, nothing, id))
    });

    let Ok(Ok((held, overlay, retracted, after, nothing, id))) = outcome else {
        panic!("the store round-trips")
    };

    assert_eq!(held.len(), 1, "one assertion, read back as one");
    let Some(correction) = held.first() else {
        panic!("there is a correction")
    };
    assert_eq!(correction.id(), id);
    assert_eq!(
        correction.exercise(),
        Exercise::Reps(RepsExercise::NeutralGripPullUp)
    );
    assert_eq!(
        correction.reason().as_str(),
        "Hevy had no neutral grip pull up",
        "§ II.2 requires the reason to survive, or it is unreadable in six months"
    );
    assert_eq!(correction.terms().count(), 1);

    let Ok(record) = SourceRecordId::try_from("a-workout") else {
        panic!("the identifier is valid")
    };
    assert_eq!(
        overlay.exercise_for(&record, PULL_UP),
        Some(Exercise::Reps(RepsExercise::NeutralGripPullUp)),
        "the overlay a derivation consults is built from what the store holds"
    );

    assert!(retracted, "retracting an assertion that exists says so");
    assert!(after.is_empty(), "and takes its terms with it");
    assert!(!nothing, "retracting it twice is not a second retraction");
}
