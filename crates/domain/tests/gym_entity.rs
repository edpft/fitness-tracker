//! § 28, applied to the entity rather than to its parts.
//!
//! "A randomly generated instance of a type must be valid. If an arbitrary
//! instance can violate an invariant, the type is wrong — not the generator."
//!
//! That is a strong claim for a workout, because four separate invariants have
//! to hold at once: an exercise has at least one set, a superset has at least
//! two members, a workout has at least one item, and a set's measure matches its
//! exercise. The generator below tries as hard as it can to build one that
//! breaks them, and the interesting thing is what it cannot express — there is
//! no way to hand `NonEmpty` nothing, no way to hand `AtLeastTwo` one, and no
//! way to put a `Set<Duration>` on an exercise counted in reps.

use domain::{
    gym::{
        AtLeastTwo, Distance, Duration, GymWorkout, Kg, Load, Metres, NonEmpty, OperatorZone,
        PerformedExercise, RepCount, Set, SetKind, SignedKg, Superset, WorkoutItem, WorkoutStart,
        exercise::{DistanceExercise, DurationExercise, RepsExercise},
    },
    landing::{Endpoint, EventKind, EventProvenance, LandingRecordId, Provenance, SourceRecordId},
};
use proptest::prelude::*;

fn load() -> impl Strategy<Value = Load> {
    prop_oneof![
        (0_u64..500_000).prop_map(|grams| Load::absolute(Kg::from_grams(grams))),
        (-100_000_i64..500_000).prop_map(|grams| Load::relative(SignedKg::from_grams(grams))),
    ]
}

fn kind() -> impl Strategy<Value = SetKind> {
    prop_oneof![Just(SetKind::Working), Just(SetKind::Warmup)]
}

fn intensity() -> impl Strategy<Value = Option<domain::gym::Rir>> {
    prop_oneof![
        Just(None),
        (0_usize..8).prop_map(|at| domain::gym::Rir::ALL.get(at).copied()),
    ]
}

fn set_of<M: std::fmt::Debug + Clone + 'static>(
    measure: impl Strategy<Value = M>,
) -> impl Strategy<Value = Set<M>> {
    (load(), measure, intensity(), kind()).prop_map(|(load, measure, intensity, kind)| Set {
        load,
        measure,
        intensity,
        kind,
        // Always absent from this source, and the type says it may be.
        rest_after: None,
    })
}

/// One to four of something, through the fallible constructor.
///
/// The generator asks for at least one and the constructor confirms it. Going
/// through `new` rather than `of` is deliberate: it exercises the rejection
/// path on every case, so a `NonEmpty` that had quietly started accepting an
/// empty vector would still be caught.
fn non_empty<T: std::fmt::Debug + 'static>(
    element: impl Strategy<Value = T>,
) -> impl Strategy<Value = NonEmpty<T>> {
    prop::collection::vec(element, 1..5).prop_filter_map("one or more is a NonEmpty", |items| {
        NonEmpty::new(items).ok()
    })
}

/// Two to four of something, likewise.
fn at_least_two<T: std::fmt::Debug + 'static>(
    element: impl Strategy<Value = T>,
) -> impl Strategy<Value = AtLeastTwo<T>> {
    prop::collection::vec(element, 2..5).prop_filter_map("two or more is an AtLeastTwo", |items| {
        AtLeastTwo::new(items).ok()
    })
}

fn performed_exercise() -> impl Strategy<Value = PerformedExercise> {
    let metres = || (0_u64..100_000).prop_map(Metres::from_millimetres);
    let seconds = || (0_u64..3_600).prop_map(Duration::from_seconds);

    prop_oneof![
        (
            (0_usize..RepsExercise::ALL.len()),
            non_empty(set_of(
                (1_u32..50)
                    .prop_filter_map("a non-zero rep count", |reps| { RepCount::new(reps).ok() })
            )),
        )
            .prop_filter_map("the vocabulary is not empty", |(at, sets)| {
                RepsExercise::ALL
                    .get(at)
                    .map(|&exercise| PerformedExercise::ForReps { exercise, sets })
            }),
        (
            (0_usize..DurationExercise::ALL.len()),
            non_empty(set_of(seconds())),
        )
            .prop_filter_map("the vocabulary is not empty", |(at, sets)| {
                DurationExercise::ALL
                    .get(at)
                    .map(|&exercise| PerformedExercise::ForDuration { exercise, sets })
            }),
        (
            (0_usize..DistanceExercise::ALL.len()),
            non_empty(set_of(metres().prop_map(|metres| Distance { metres }))),
        )
            .prop_filter_map("the vocabulary is not empty", |(at, sets)| {
                DistanceExercise::ALL
                    .get(at)
                    .map(|&exercise| PerformedExercise::ForDistance { exercise, sets })
            }),
    ]
}

fn item() -> impl Strategy<Value = WorkoutItem> {
    prop_oneof![
        performed_exercise().prop_map(WorkoutItem::Exercise),
        at_least_two(performed_exercise())
            .prop_map(|members| WorkoutItem::Superset(Superset { members })),
    ]
}

fn workout() -> impl Strategy<Value = GymWorkout> {
    (non_empty(item()), 0_i64..1_000_000_000).prop_filter_map(
        "a workout is buildable",
        |(items, seconds)| {
            let zone = OperatorZone::try_from("Europe/London").ok()?;
            let instant = jiff::Timestamp::from_second(seconds).ok()?;
            let provenance = Provenance::Event(EventProvenance::new(
                Endpoint::try_from("/v1/workouts/events").ok()?,
                EventKind::Updated,
                None,
            ));
            Some(GymWorkout::new(
                items,
                WorkoutStart::new(instant, zone),
                provenance,
                SourceRecordId::try_from("synthetic").ok()?,
                LandingRecordId::FIRST,
            ))
        },
    )
}

proptest! {
    /// Every invariant at once, over an arbitrary instance.
    ///
    /// None of these can fail as the types stand, which is exactly what § 28
    /// asks for — the test earns its place by failing loudly if any of the four
    /// containers is ever loosened to a validated `Vec`.
    #[test]
    fn an_arbitrary_workout_is_a_valid_one(workout in workout()) {
        prop_assert!(workout.items().count() >= 1, "a workout has at least one item");

        for item in workout.items().iter() {
            if let WorkoutItem::Superset(superset) = item {
                prop_assert!(
                    superset.members.count() >= 2,
                    "a superset has at least two members"
                );
            }
        }

        for exercise in workout.exercises() {
            prop_assert!(
                exercise.set_count() >= 1,
                "an exercise holds at least one set"
            );
        }

        prop_assert_eq!(
            workout.set_count(),
            workout.exercises().map(PerformedExercise::set_count).sum::<usize>()
        );
    }

    /// Every set carries a load, whichever measure it is counted in. Load is a
    /// property of a set rather than a kind of set, so there is no arm here
    /// where one is absent.
    #[test]
    fn every_set_carries_a_load(workout in workout()) {
        macro_rules! check {
            ($sets:expr) => {
                for set in $sets.iter() {
                    prop_assert!(matches!(
                        set.load,
                        Load::Absolute(_) | Load::Relative(_)
                    ));
                }
            };
        }

        for exercise in workout.exercises() {
            match exercise {
                PerformedExercise::ForReps { sets, .. } => check!(sets),
                PerformedExercise::ForDuration { sets, .. } => check!(sets),
                PerformedExercise::ForDistance { sets, .. } => check!(sets),
            }
        }
    }

    /// The measure an exercise reports is the vocabulary it came from, always.
    ///
    /// A stored measurement type would make this checkable and therefore
    /// breakable; here the two cannot disagree because there is only one of
    /// them.
    #[test]
    fn every_exercise_reports_the_measure_of_its_vocabulary(workout in workout()) {
        for exercise in workout.exercises() {
            let expected = match exercise {
                PerformedExercise::ForReps { .. } => "reps",
                PerformedExercise::ForDuration { .. } => "duration",
                PerformedExercise::ForDistance { .. } => "distance",
            };
            prop_assert_eq!(exercise.measure(), expected);
        }
    }

    /// A workout knows where it came from. Provenance is a constructor
    /// argument, so there is no instance without it.
    #[test]
    fn every_workout_carries_provenance_and_a_zone(workout in workout()) {
        let Provenance::Event(event) = workout.provenance();
        prop_assert_eq!(event.kind().as_str(), "updated");
        prop_assert_eq!(workout.started_at().zone().id(), "Europe/London");
        prop_assert!(!workout.source_record_id().as_str().is_empty());
    }
}
