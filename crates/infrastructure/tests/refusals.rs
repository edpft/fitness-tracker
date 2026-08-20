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

use domain::gym::{PerformedExercise, RefusalLocus, RefusalReason};
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
        ("non-contiguous-grouping", 1),
        ("single-member-grouping", 1),
    ]
    .into_iter()
    .collect();

    assert_eq!(by_reason(&produced), expected);
    assert_eq!(produced.refusals.len(), 2, "two groupings, and no set");
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

    // **Only one kind is left, and that is the point of US2.** The corpus's one
    // unmodelled case — 95 kg for zero reps — is modelled now, so everything still
    // refused is wrong data. `RefusalKind::Unmodelled` stays in the vocabulary
    // reachable but unreached: the next thing the domain cannot hold needs it, and
    // an operator has to be able to tell "refine the model" from "fix the record".
    let expected: BTreeMap<&str, usize> = std::iter::once(("wrong data", 2)).collect();
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

// **The one genuine gap is no longer a gap.** 95 kg for zero repetitions is a
// failed attempt now rather than a refusal, so the test that asserted it as
// unmodelled has gone. `tests/failed_attempt.rs` asserts the outcome and decision
// record `0007` argues the reversal. What remains refused is the two malformed
// groupings, which the tests either side of this cover.

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
