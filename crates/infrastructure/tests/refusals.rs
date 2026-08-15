//! What the domain will not accept, and why.
//!
//! § 37 is not satisfied by translating what fits. One set and two groupings in
//! the corpus do not translate, and telling apart what is wrong data from what
//! the model does not hold is the whole value — unavailable if the refusal is a
//! stack trace or a dropped row.
//!
//! Every assertion here is over `RefusalReason`, never over rendered text. The
//! claim is that the refusals are *exactly* a known set, which is a query and
//! not a grep — and a formatted sentence would satisfy a reader while defeating
//! the assertion that matters.

mod support;

use std::collections::BTreeMap;

use domain::gym::{PerformedExercise, RefusalKind, RefusalLocus, RefusalReason};
use support::{corpus, derived};

fn by_reason(produced: &corpus::Produced) -> BTreeMap<&'static str, usize> {
    let mut counts = BTreeMap::new();
    for refusal in &produced.refusals {
        *counts.entry(refusal.reason.as_str()).or_insert(0) += 1;
    }
    counts
}

/// Scenario 5. The refusal set is exactly the named one — and, just as
/// importantly, nothing else.
///
/// A refusal outside this set is a regression. A case inside it silently
/// translating is a worse one, because it means the domain accepted something
/// it cannot express.
#[test]
fn the_refusals_are_exactly_the_named_set() {
    let produced = derived!();

    let expected: BTreeMap<&str, usize> = [
        ("zero-reps", 1),
        ("non-contiguous-grouping", 1),
        ("single-member-grouping", 1),
    ]
    .into_iter()
    .collect();

    assert_eq!(by_reason(&produced), expected);
    assert_eq!(produced.refusals.len(), 3, "one set and two groupings");
}

/// The three kinds are the deliverable. A model that cannot hold a genuine case
/// needs refining; a model that rejects a wrong record is working — and an
/// operator has to be able to tell which without opening the payload.
#[test]
fn every_refusal_says_what_to_do_about_it() {
    let produced = derived!();

    let mut kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
    for refusal in &produced.refusals {
        *kinds.entry(refusal.kind().as_str()).or_insert(0) += 1;
        assert!(
            !refusal.source_record_id.as_str().is_empty(),
            "a refusal names its record"
        );
    }

    let expected: BTreeMap<&str, usize> = [
        // The two malformed groupings.
        ("wrong data", 2),
        // The one missed attempt: 95 kg for zero reps at the top of the scale.
        ("unmodelled", 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(kinds, expected);
}

/// The two malformed groupings are malformed, and neither costs its workout.
///
/// One has members either side of a non-member; one has a single member, the
/// last exercise in its workout, where the partner was never added. Both fail
/// the definition rather than testing it, and in both cases the member
/// exercises still translate as ordinary items in their recorded order.
#[test]
fn a_malformed_grouping_does_not_cost_its_members() {
    let produced = derived!();

    let malformed: Vec<&domain::gym::Refusal> = produced
        .refusals
        .iter()
        .filter(|refusal| {
            matches!(
                refusal.reason,
                RefusalReason::NonContiguousGrouping | RefusalReason::SingleMemberGrouping
            )
        })
        .collect();
    assert_eq!(malformed.len(), 2);

    for refusal in &malformed {
        assert!(
            matches!(refusal.locus, RefusalLocus::Grouping { .. }),
            "a refused grouping names itself: {:?}",
            refusal.locus
        );
    }

    // `b6995e63` has `Running` at position 3 and `V Up` at 5, either side of a
    // non-member. Both still translate, and its workout is intact.
    let Some(workout) = corpus::workout_starting(&produced, "b6995e63-739d-4512-8b39-3de02ef9ad77")
    else {
        panic!("the non-contiguous workout still translates")
    };
    let names: Vec<&str> = workout
        .exercises()
        .map(PerformedExercise::exercise_key)
        .collect();
    assert!(names.contains(&"running"), "{names:?}");
    assert!(names.contains(&"v-up"), "{names:?}");

    // `3f9e9a6a`'s single-member group is a triceps extension, and it survives
    // as an ordinary item.
    let Some(workout) = corpus::workout_starting(&produced, "3f9e9a6a-f252-459b-9d0c-cdf28436ab27")
    else {
        panic!("the single-member workout still translates")
    };
    let names: Vec<&str> = workout
        .exercises()
        .map(PerformedExercise::exercise_key)
        .collect();
    assert!(names.contains(&"triceps-extension-cable"), "{names:?}");
}

/// The one genuine gap, kept visible.
///
/// 95 kg for zero reps at zero in reserve: an attempt that failed. It is a real
/// event and it is not a set, so no refinement of `RepCount` captures it
/// honestly — it needs an attempt, which belongs with
/// prescribed-versus-performed. Recorded as unmodelled rather than coerced.
#[test]
fn the_missed_attempt_is_refused_as_unmodelled() {
    let produced = derived!();

    let attempts: Vec<&domain::gym::Refusal> = produced
        .refusals
        .iter()
        .filter(|refusal| refusal.reason == RefusalReason::ZeroReps)
        .collect();

    assert_eq!(attempts.len(), 1, "one missed attempt in the corpus");
    let Some(attempt) = attempts.first() else {
        panic!("the attempt was just counted")
    };
    assert_eq!(attempt.kind(), RefusalKind::Unmodelled);
    assert_eq!(
        attempt.source_record_id.as_str(),
        "296ef7e3-7e16-4a38-82ce-be204c43b575"
    );
    // A front squat, and the load was fine — 95 kg on a barbell. What the
    // domain will not hold is the zero, which is why the reason is the reps and
    // not the weight.
    assert_eq!(
        attempt.exercise.map(domain::gym::Exercise::as_str),
        Some("front-squat")
    );
}

/// Scenario 10. Nothing is skipped silently: every record has exactly one
/// outcome and the numbers add up.
#[test]
fn no_record_is_both_a_workout_and_a_refusal_of_itself() {
    let produced = derived!();

    let record_level: Vec<_> = produced
        .refusals
        .iter()
        .filter(|refusal| refusal.locus == RefusalLocus::Record)
        .map(|refusal| refusal.landed_as)
        .collect();
    assert!(
        record_level.is_empty(),
        "no record in the corpus refuses whole: {record_level:?}"
    );

    for refusal in &produced.refusals {
        let has_workout = produced
            .workouts
            .iter()
            .any(|workout| workout.landed_as() == refusal.landed_as);
        assert!(
            has_workout,
            "every refusal in the corpus sits inside a workout that translated"
        );
    }

    assert!(produced.summary.reconciles(), "{:?}", produced.summary);
}
