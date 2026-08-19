//! A failed attempt is an outcome, not a refusal.
//!
//! User story 2, and it reverses what 002 shipped. `0002` refused a zero-rep set
//! as unmodelled and said why in its own CHECK — "a rep count of zero is an
//! attempt, not a set" — which was right about zero not being a count and wrong
//! about where the case goes. The prescribed side now exists to give a failure
//! meaning: the negative gate in `docs/primary-lift-progression.md` detects a
//! stall from a failure, so a failure the layer will not represent is a stall the
//! programme cannot see.
//!
//! **The discriminator is the rep count, never the source's set type.** Hevy
//! marks 77 sets `failure` in this corpus and means "taken to failure" by it;
//! exactly one of the 77 is a failed attempt. Keying on the type would misfile 76
//! completed sets, every one of which currently contributes to a volume total.

mod support;

use std::collections::BTreeMap;

use domain::gym::{
    GymWorkout, Kg, Load, Performed, PerformedExercise, Refusal, RefusalKind, RepCount, Set,
    SetKind, exercise::RepsExercise,
};
use support::{corpus, derived};

/// Every set of every reps-counted exercise, with the exercise it belongs to.
fn rep_sets(workouts: &[GymWorkout]) -> Vec<(RepsExercise, Set<RepCount>)> {
    let mut found = Vec::new();
    for workout in workouts {
        for exercise in workout.exercises() {
            if let PerformedExercise::ForReps { exercise, sets } = exercise {
                for set in sets.iter() {
                    found.push((*exercise, *set));
                }
            }
        }
    }
    found
}

/// US2-1: the 95kg set of 2026-07-03 is a failed attempt.
///
/// The load was never the problem — 95kg on a barbell is an ordinary number. What
/// the model could not hold was the zero, and now it does not have to: the set
/// carries the load and no count at all.
#[test]
fn zero_reps_becomes_a_failed_attempt() {
    let produced = derived!();

    let failures: Vec<(RepsExercise, Set<RepCount>)> = rep_sets(&produced.workouts)
        .into_iter()
        .filter(|(_, set)| matches!(set.outcome, Performed::Failed))
        .collect();

    assert_eq!(failures.len(), 1, "one failed attempt in the corpus");
    let Some((exercise, set)) = failures.first() else {
        panic!("the attempt was just counted")
    };
    assert_eq!(exercise.as_str(), "front-squat");
    assert_eq!(set.load, Load::Absolute(Kg::from_grams(95_000)));
    assert_eq!(set.kind, SetKind::Working);
    assert!(
        set.outcome.completed().is_none(),
        "a failed attempt yields no quantity"
    );

    // And it is no longer refused. `RefusalReason::ZeroReps` is gone, so this is
    // asserted through the kind: nothing in the corpus is unmodelled any more.
    let unmodelled: Vec<&Refusal> = produced
        .refusals
        .iter()
        .filter(|refusal| refusal.kind() == RefusalKind::Unmodelled)
        .collect();
    assert!(
        unmodelled.is_empty(),
        "the one unmodelled case is modelled now: {unmodelled:?}"
    );
    assert_eq!(
        produced.refusals.len(),
        2,
        "two malformed groupings, and nothing else"
    );
}

/// US2-2: the source's `failure` set type is not the discriminator.
///
/// **The guard against the tempting shortcut.** Hevy's type means "taken to
/// failure", which is a note about effort and sits on 77 sets here. Exactly one of
/// them failed. A translator keyed on the type would file 76 completed working
/// sets as attempts and silently remove their volume from every total.
#[test]
fn the_failure_set_type_is_not_the_discriminator() {
    let produced = derived!();

    // Counted in the raw payloads, so this is what the source said rather than
    // what we made of it.
    let Ok(records) = corpus::records() else {
        panic!("the corpus fixture loads")
    };
    let typed_failure: usize = records
        .iter()
        .map(|landed| {
            let payload = landed.record().payload().as_bytes();
            payload
                .windows(SET_TYPE_FAILURE.len())
                .filter(|window| *window == SET_TYPE_FAILURE)
                .count()
        })
        .sum();
    assert_eq!(typed_failure, 77, "sets the source marked `failure`");

    let attempts = rep_sets(&produced.workouts)
        .into_iter()
        .filter(|(_, set)| matches!(set.outcome, Performed::Failed))
        .count();
    assert_eq!(attempts, 1, "of which one is a failed attempt");
    assert_eq!(
        typed_failure - attempts,
        76,
        "the other 76 translate as completed sets and keep their volume"
    );
}

const SET_TYPE_FAILURE: &[u8] = br#""type":"failure""#;

/// US2-3 and SC-007: a failure contributes to no total, count or estimate.
///
/// **Checked as a diff rather than against a value.** A hard-coded expected
/// tonnage would pass even if the failure were being counted and the constant had
/// been updated to match. So every figure is computed twice — over the record as
/// it now stands, and over the same record with the attempt taken out — and the
/// two must agree. The one figure that legitimately moves is the number of sets
/// the layer holds, because a set that was refused is now recorded.
#[test]
fn a_failure_is_not_a_quantity() {
    let produced = derived!();
    let all = rep_sets(&produced.workouts);
    let without: Vec<(RepsExercise, Set<RepCount>)> = all
        .iter()
        .filter(|(_, set)| !matches!(set.outcome, Performed::Failed))
        .copied()
        .collect();

    assert_eq!(
        all.len() - without.len(),
        1,
        "exactly one set is being taken out"
    );

    // Tonnage, per exercise. The failure has a load, so this is the figure most
    // likely to absorb it.
    assert_eq!(
        tonnage(&all),
        tonnage(&without),
        "no tonnage moves when the attempt is removed"
    );
    // Repetitions, per exercise.
    assert_eq!(
        repetitions(&all),
        repetitions(&without),
        "no repetition count moves"
    );
    // The heaviest load with a count behind it — the maximum estimate. 95kg was
    // on the bar and did not go up, so the heaviest front squat is still 90kg.
    let heaviest = heaviest_completed(&all);
    assert_eq!(heaviest, heaviest_completed(&without));
    let Ok(front_squat) = RepsExercise::try_from("front-squat".to_owned()) else {
        panic!("the front squat is in the vocabulary")
    };
    assert_eq!(
        heaviest.get(&front_squat).copied(),
        Some(Kg::from_grams(90_000)),
        "the attempt is not a lift"
    );

    // What does change, and the only thing that does: the layer holds one more
    // set than it did when the attempt was refused.
    let sets: usize = produced
        .workouts
        .iter()
        .map(domain::gym::GymWorkout::set_count)
        .sum();
    assert_eq!(sets, 3_779, "every set in the corpus is now recorded");
    assert_eq!(
        all.iter()
            .filter(|(_, set)| set.outcome.completed().is_some())
            .count()
            + all
                .iter()
                .filter(|(_, set)| matches!(set.outcome, Performed::Failed))
                .count(),
        all.len(),
        "every set is one or the other, and there is no third state"
    );
}

fn tonnage(sets: &[(RepsExercise, Set<RepCount>)]) -> BTreeMap<RepsExercise, u64> {
    let mut totals = BTreeMap::new();
    for (exercise, set) in sets {
        let Some(reps) = set.outcome.completed() else {
            continue;
        };
        let Load::Absolute(mass) = set.load else {
            continue;
        };
        *totals.entry(*exercise).or_insert(0) += mass.as_grams() * u64::from(reps.as_u32());
    }
    totals
}

fn repetitions(sets: &[(RepsExercise, Set<RepCount>)]) -> BTreeMap<RepsExercise, u32> {
    let mut totals = BTreeMap::new();
    for (exercise, set) in sets {
        if let Some(reps) = set.outcome.completed() {
            *totals.entry(*exercise).or_insert(0) += reps.as_u32();
        }
    }
    totals
}

fn heaviest_completed(sets: &[(RepsExercise, Set<RepCount>)]) -> BTreeMap<RepsExercise, Kg> {
    let mut heaviest = BTreeMap::new();
    for (exercise, set) in sets {
        if set.outcome.completed().is_none() {
            continue;
        }
        let Load::Absolute(mass) = set.load else {
            continue;
        };
        heaviest
            .entry(*exercise)
            .and_modify(|held: &mut Kg| {
                if mass > *held {
                    *held = mass;
                }
            })
            .or_insert(mass);
    }
    heaviest
}

/// US2-4: a failure and an absence are distinguishable.
///
/// The sharpest thing the projection found (research D9): a failed attempt is a
/// load that was on the bar with no count behind it, and an absence is nothing at
/// all. Both yield `None` when asked for a quantity, which is why the difference
/// has to be visible somewhere else — and it is, in whether a set exists.
#[test]
fn a_failure_is_not_an_absence() {
    let produced = derived!();
    let all = rep_sets(&produced.workouts);

    let Some((_, attempt)) = all
        .iter()
        .find(|(_, set)| matches!(set.outcome, Performed::Failed))
    else {
        panic!("the corpus holds one failed attempt")
    };

    // Present, and carrying the fact that survived: the load.
    assert_eq!(attempt.load, Load::Absolute(Kg::from_grams(95_000)));
    assert!(attempt.outcome.completed().is_none());

    // An absence has no set to ask. The front squat was performed on 2026-07-03
    // and never on the 4th, and the difference is that there is nothing there —
    // not a set whose measure is missing.
    let on_the_fourth = produced
        .workouts
        .iter()
        .filter(|workout| {
            workout.started_at().wall_clock().date() == jiff::civil::Date::constant(2026, 7, 4)
        })
        .count();
    assert_eq!(on_the_fourth, 0, "nothing was performed on the 4th");
}

/// US2-5: re-derivation over unchanged raw is identical.
///
/// § 7. The new arm reads only the payload, so this would break if it consulted
/// anything that moves — a clock, an insertion order, the previous derivation.
#[test]
fn re_derivation_is_unaffected() {
    let first = derived!();
    let second = derived!();

    assert_eq!(
        first.workouts, second.workouts,
        "the workouts are identical"
    );
    assert_eq!(
        first.refusals.len(),
        second.refusals.len(),
        "and so are the refusals"
    );
}
