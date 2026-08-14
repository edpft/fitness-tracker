//! A withdrawn workout is absent, not marked.
//!
//! User story 3. § II.3's retraction paragraph, which this feature is the
//! reason for: a source event withdrawing a record it previously served leaves
//! that record with no normalised entity, because a withdrawn record is not
//! something the source is still saying.
//!
//! The corpus can only test half of it. Its single `deleted` record names a
//! workout created and deleted between two extraction runs, so no `updated`
//! record for it was ever landed and it withdraws nothing. The other half — a
//! deletion that actually removes something — is synthetic, and is run in both
//! landing orders because that is what separates an absorbing retraction from
//! latest-wins.

mod support;

use application::NormalisationError;
use domain::gym::RefusalReason;
use support::{corpus, derived};

/// The record already in raw: a tombstone for a workout never landed.
const TOMBSTONE: &str = "93d50b8d-f806-4042-959f-263dbb6a53f7";

/// Scenario 8, first half. The tombstone withdraws nothing, fails nothing, and
/// is not a refusal — nothing about it was rejected.
#[test]
fn a_tombstone_for_a_workout_never_landed_withdraws_nothing() {
    let produced = derived!();

    assert_eq!(produced.workouts.len(), 163);
    assert_eq!(produced.summary.retractions_applied.as_u64(), 1);
    assert_eq!(
        produced.summary.workouts_withdrawn.as_u64(),
        0,
        "it names a workout that was never landed"
    );

    // FR-027. A retraction is a working source event, and putting it in the
    // operator's list of things to fix would be reporting the feed doing its
    // job as a defect.
    let named_in_a_refusal = produced
        .refusals
        .iter()
        .any(|refusal| refusal.source_record_id.as_str() == TOMBSTONE);
    assert!(!named_in_a_refusal, "a retraction is not a refusal");

    let unreadable = produced
        .refusals
        .iter()
        .filter(|refusal| matches!(refusal.reason, RefusalReason::UnreadablePayload { .. }))
        .count();
    assert_eq!(unreadable, 0, "the body-less shape reads fine");
}

/// Scenario 8, second half. A deletion for a workout that *was* landed leaves
/// it with no entity — and does so whichever order the two records arrive in.
///
/// Latest-wins would pass the first of these and fail the second, which is why
/// both are here.
#[test]
fn a_deletion_withdraws_the_workout_it_names_in_either_order() {
    let Ok(fixture) = corpus::derivation() else {
        panic!("the corpus fixture loads")
    };
    let baseline = derived!();

    // Give the tombstone something to withdraw: an `updated` record for the
    // identifier it names, built from a real workout so the body is genuine.
    let Ok(withdrawable) = corpus::with_synthetic_update_for(&fixture, TOMBSTONE) else {
        panic!("the synthetic record builds")
    };

    for reversed in [false, true] {
        let produced = derived!(withdrawable, reversed);

        assert_eq!(
            produced.summary.records_read.as_u64(),
            165,
            "the corpus plus one synthetic update"
        );
        assert_eq!(
            produced.workouts.len(),
            163,
            "the workout the retraction names is the one absent (reversed: {reversed})"
        );
        assert_eq!(produced.summary.workouts_withdrawn.as_u64(), 1);
        assert_eq!(produced.summary.retractions_applied.as_u64(), 1);
        assert!(produced.summary.reconciles(), "{:?}", produced.summary);

        let withdrawn = produced
            .workouts
            .iter()
            .any(|workout| workout.source_record_id().as_str() == TOMBSTONE);
        assert!(!withdrawn, "the withdrawn workout has no entity");

        // Every other workout is untouched. A retraction removes the record it
        // names and nothing else.
        let mut theirs: Vec<_> = produced.workouts.iter().map(domain::gym::GymWorkout::landed_as).collect();
        let mut ours: Vec<_> = baseline.workouts.iter().map(domain::gym::GymWorkout::landed_as).collect();
        theirs.sort_unstable();
        ours.sort_unstable();
        assert_eq!(theirs, ours, "no other workout is affected");
    }
}

/// § 10 stays where it is. Two records sharing a source identifier are the same
/// source contradicting itself, and the later supersedes — but that is a
/// question for the layer that can see every source, so both stand here.
///
/// Synthetic, because the corpus holds 164 distinct identifiers and not one
/// re-serve.
#[test]
fn two_records_for_one_workout_both_stand() {
    let Ok(fixture) = corpus::derivation() else {
        panic!("the corpus fixture loads")
    };
    let Ok(reserved) = corpus::with_reserved_first_workout(&fixture) else {
        panic!("the synthetic re-serve builds")
    };

    let produced = derived!(reserved, false);

    assert_eq!(produced.workouts.len(), 164, "both records produce a workout");
    let Some(first) = fixture.records.first() else {
        panic!("the corpus is not empty")
    };
    let shared = first.source_record_id().as_str();
    let sharing = produced
        .workouts
        .iter()
        .filter(|workout| workout.source_record_id().as_str() == shared)
        .count();
    assert_eq!(sharing, 2, "neither is marked current and neither is removed");
}

/// Scenario 9. FR-017 and the one exception to FR-024.
///
/// A template the mapping does not cover is a gap in *our* vocabulary, not a
/// problem with the data — so it stops the run naming itself, and nothing is
/// written. The previous derivation is left standing rather than half-replaced,
/// which is what the single transaction is for.
#[test]
fn an_unmapped_template_stops_the_run_and_writes_nothing() {
    let Ok(fixture) = corpus::derivation() else {
        panic!("the corpus fixture loads")
    };
    let Ok(with_gap) = corpus::with_unmapped_template(&fixture, "NOT-A-TEMPLATE") else {
        panic!("the synthetic record builds")
    };

    let Ok(outcome) = corpus::block_on(with_gap.run(false)) else {
        panic!("a runtime is available")
    };

    match outcome {
        Err(NormalisationError::UnmappedExercise { template_id, .. }) => {
            assert_eq!(template_id, "NOT-A-TEMPLATE", "the run names the gap");
        }
        Err(other) => panic!("the run failed for the wrong reason: {other}"),
        Ok(_) => panic!("an unmapped template must not translate around the gap"),
    }
}
