//! Deriving the normalised layer from the landed corpus.
//!
//! User story 1. These are the feature's steering signal (§ 29): they drive the
//! use case through its real ports with the real Hevy translator, against the
//! 164 records every figure in the model of record was checked against.
//!
//! They live in `infrastructure` rather than `application` because they need
//! the translator, and `application` may not depend on the ring above it. That
//! is the hexagon working rather than a compromise: the use case is generic
//! over its ports, so the test that supplies real ones belongs where real ones
//! live.

mod support;

use std::collections::BTreeMap;

use domain::gym::{Load, PerformedExercise, SetKind};
use infrastructure::hevy::mapping;
use support::{corpus, derived};

/// Scenario 1. The counts the model of record produced on paper, reproduced by
/// code — which is the whole point of the feature.
#[test]
fn the_corpus_translates_to_the_model_of_records_figures() {
    let produced = derived!();

    let entries: usize = produced
        .workouts
        .iter()
        .map(|workout| workout.exercises().count())
        .sum();
    let sets: usize = produced
        .workouts
        .iter()
        .map(domain::gym::GymWorkout::set_count)
        .sum();
    let supersets: usize = produced
        .workouts
        .iter()
        .map(domain::gym::GymWorkout::superset_count)
        .sum();

    assert_eq!(produced.workouts.len(), 163, "workouts");
    assert_eq!(entries, 1_135, "every landed exercise entry translates");
    assert_eq!(supersets, 334, "every well-formed grouping is a superset");

    // One set in the corpus does not translate: 95 kg for zero reps, an attempt
    // that failed. It is a real event and it is not a set, and nothing else is
    // refused — the model rejects exactly the thing it has no shape for.
    assert_eq!(sets, 3_778, "of 3,779 sets translate");
    assert_eq!(produced.refusals.len(), 3, "one set and two groupings");
}

/// SC-003. Every template the corpus holds is covered, so no derivation of it
/// can hit FR-017's failure. Asserted over the payloads rather than over what
/// translated, because an entry whose sets all refused still had to resolve.
#[test]
fn every_landed_template_resolves() {
    let Ok(templates) = corpus::landed_template_ids() else {
        panic!("the corpus fixture loads")
    };

    let unmapped: Vec<&String> = templates
        .iter()
        .filter(|id| mapping::lookup(id).is_none())
        .collect();

    assert!(unmapped.is_empty(), "unmapped templates: {unmapped:?}");
    assert_eq!(templates.len(), 134, "distinct templates in the corpus");
}

/// Scenario 2. § 7's re-derivation, three ways: twice over the same input,
/// then over the input reversed.
///
/// The reversal is what separates an absorbing retraction from latest-wins. A
/// derivation whose result depends on the order raw is read in is one that has
/// resolved supersession by position.
#[test]
fn re_derivation_is_identical_and_order_does_not_matter() {
    let Ok(fixture) = corpus::derivation() else {
        panic!("the corpus fixture loads")
    };

    let Ok(Ok(first)) = corpus::block_on(fixture.run(false)) else {
        panic!("the first derivation succeeds")
    };
    let Ok(Ok(second)) = corpus::block_on(fixture.run(false)) else {
        panic!("the second derivation succeeds")
    };
    let Ok(Ok(reversed)) = corpus::block_on(fixture.run(true)) else {
        panic!("the reversed derivation succeeds")
    };

    assert_eq!(first.workouts, second.workouts, "workouts re-derive equal");
    assert_eq!(first.refusals, second.refusals, "refusals re-derive equal");

    // Reversed, the sequence differs but the set of workouts does not.
    let by_record = |produced: &corpus::Produced| {
        produced
            .workouts
            .iter()
            .map(|workout| (workout.landed_as(), workout.set_count()))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(
        by_record(&first),
        by_record(&reversed),
        "read order does not change what is derived"
    );
}

/// Scenario 3. Identity comes from the mapping, not the label — and the whole
/// argument for the load model rests on this collapse holding.
#[test]
fn assisted_and_unassisted_forms_are_one_series() {
    let produced = derived!();

    let mut loads: BTreeMap<&str, Vec<i64>> = BTreeMap::new();
    for exercise in produced
        .workouts
        .iter()
        .flat_map(domain::gym::GymWorkout::exercises)
    {
        let PerformedExercise::ForReps { exercise, sets } = exercise else {
            continue;
        };
        for set in sets.iter() {
            if let Load::Relative(delta) = set.load {
                loads
                    .entry(exercise.as_str())
                    .or_default()
                    .push(delta.as_grams());
            }
        }
    }

    let Some(pull_ups) = loads.get("pull-up") else {
        panic!("the corpus holds pull-ups")
    };
    // 97 plain + 159 assisted + 3 banded, one exercise, one series.
    assert_eq!(
        pull_ups.len(),
        259,
        "pull-up sets across all three templates"
    );
    assert!(
        pull_ups.iter().any(|grams| *grams < 0),
        "assistance is carried as negative load"
    );
    assert!(
        pull_ups.contains(&0),
        "a plain pull-up is a real zero, not an absence"
    );

    let Some(dips) = loads.get("chest-dip") else {
        panic!("the corpus holds chest dips")
    };
    // 84 plain + 277 assisted. The 30 assisted sets recording zero translate
    // too, as plain bodyweight, so the total is the full 361.
    assert_eq!(dips.len(), 361, "chest dip sets across both templates");
}

/// Scenario 6. Absence stays absent. Not zero, not carried forward from a
/// neighbouring set, and not reconstructed from a linked routine (§ 11, § 37).
#[test]
fn absence_is_absence() {
    let produced = derived!();

    let mut with_intensity = 0_usize;
    let mut without_intensity = 0_usize;
    let mut warmups = 0_usize;
    let mut with_rest = 0_usize;

    macro_rules! tally {
        ($sets:expr) => {
            for set in $sets.iter() {
                if set.intensity.is_some() {
                    with_intensity += 1;
                } else {
                    without_intensity += 1;
                }
                if set.kind == SetKind::Warmup {
                    warmups += 1;
                }
                if set.rest_after.is_some() {
                    with_rest += 1;
                }
            }
        };
    }

    for exercise in produced
        .workouts
        .iter()
        .flat_map(domain::gym::GymWorkout::exercises)
    {
        match exercise {
            PerformedExercise::ForReps { sets, .. } => tally!(sets),
            PerformedExercise::ForDuration { sets, .. } => tally!(sets),
            PerformedExercise::ForDistance { sets, .. } => tally!(sets),
        }
    }

    // 2,415 sets in the corpus record an intensity. One sits on the zero-rep
    // attempt, which refuses, so 2,414 reach the normalised layer.
    assert_eq!(with_intensity, 2_414, "sets carrying an intensity");
    assert_eq!(with_intensity + without_intensity, 3_778, "sets in total");
    // No positional rule reconstructs these: the corpus opens workouts with
    // heavy bridging singles tagged as warm-ups.
    assert_eq!(warmups, 361, "warm-up sets");
    assert_eq!(
        with_rest, 0,
        "this source records no rest, and none is invented"
    );
}

/// Scenario 7. § II.3's wall clock, across both switchovers.
///
/// That no timestamp lacks a zone is a fact about the type rather than
/// something to test — `WorkoutStart` has no constructor taking an instant
/// alone. What can fail, and so is what is asserted, is the wall clock.
#[test]
fn the_wall_clock_survives_both_switchovers() {
    let produced = derived!();

    // Two real workouts, one either side of a switchover. Both were trained in
    // the evening, and both must read as the evening — which is the whole of
    // "8pm stays 8pm".
    let summer = corpus::workout_starting(&produced, "6d86320a-a93b-4b93-bc1a-925fb9690c17");
    let winter = corpus::workout_starting(&produced, "a1806fe2-2074-4193-89f0-a8b9e297003d");

    let Some(summer) = summer else {
        panic!("the June workout is in the corpus")
    };
    let Some(winter) = winter else {
        panic!("the December workout is in the corpus")
    };

    // 19:03 UTC in June is 20:03 in London; 20:33 UTC in December is 20:33.
    // An offset stored at write time would have got one of these wrong.
    assert_eq!(summer.started_at().wall_clock().hour(), 20, "June, BST");
    assert_eq!(winter.started_at().wall_clock().hour(), 20, "December, GMT");
    assert_eq!(summer.started_at().wall_clock().offset().seconds(), 3_600);
    assert_eq!(winter.started_at().wall_clock().offset().seconds(), 0);

    for workout in &produced.workouts {
        assert_eq!(
            workout.started_at().zone().id(),
            "Europe/London",
            "every workout carries the declared zone"
        );
    }
}

/// The zone is configuration, and changing it changes how instants read
/// without changing the instants. That is the difference between carrying a
/// zone and carrying an offset, and it is why § II.3 insists on the former.
#[test]
fn a_different_declared_zone_moves_the_wall_clock_and_not_the_instant() {
    let Ok(fixture) = corpus::derivation() else {
        panic!("the corpus fixture loads")
    };
    let Ok(elsewhere) = domain::gym::OperatorZone::try_from("Australia/Sydney") else {
        panic!("Sydney is an IANA zone")
    };

    let Ok(Ok(london)) = corpus::block_on(fixture.run(false)) else {
        panic!("the London derivation succeeds")
    };
    let Ok(Ok(sydney)) = corpus::block_on(fixture.run_in(elsewhere, false)) else {
        panic!("the Sydney derivation succeeds")
    };

    assert_eq!(london.workouts.len(), sydney.workouts.len());
    let mut moved = 0_usize;
    for (here, there) in london.workouts.iter().zip(&sydney.workouts) {
        assert_eq!(
            here.started_at().instant(),
            there.started_at().instant(),
            "the instant is what the source served, and is untouched"
        );
        if here.started_at().wall_clock().hour() != there.started_at().wall_clock().hour() {
            moved += 1;
        }
    }
    assert_eq!(moved, london.workouts.len(), "every wall clock moves");
}

/// Scenario 10. Every record accounted for, exactly once (FR-005, SC-005).
#[test]
fn every_record_is_accounted_for() {
    let produced = derived!();
    let summary = produced.summary;

    assert_eq!(summary.records_read.as_usize(), 164, "records read");
    assert_eq!(summary.workouts_written.as_usize(), 163, "workouts written");
    assert_eq!(summary.retractions_read.as_usize(), 1, "retractions served");
    assert_eq!(
        summary.workouts_retracted.as_usize(),
        0,
        "the one retraction names a workout that was never landed"
    );
    assert_eq!(
        summary.records_refused.as_usize(),
        0,
        "records refused whole"
    );
    assert!(summary.reconciles(), "the numbers add up: {summary:?}");
}
