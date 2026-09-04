//! § 28 over the prescribed entity (T027).
//!
//! The § 24 work in this feature is that "prescribes nothing" is unconstructible
//! and a superset cannot hold one member. Both are asserted here over arbitrary
//! instances, because an invariant a generator can violate is a wrong type
//! rather than a wrong generator.

use domain::gym::{
    Duration, Load, RepCount, Rir, SignedKg,
    exercise::{DurationExercise, RepsExercise},
    sequence::{AtLeastTwo, NonEmpty},
};
use domain::prescription::{
    Prescribed, PrescribedExercise, PrescribedItem, PrescribedSet, PrescribedSuperset, SlotId,
    SupersetMember, Target, WorkoutShape,
};
use proptest::prelude::*;

fn load() -> impl Strategy<Value = Load> {
    prop_oneof![
        (0_u64..300_000).prop_map(|grams| Load::Absolute(domain::gym::Kg::from_grams(grams))),
        (-40_000_i64..40_000).prop_map(|grams| Load::Relative(SignedKg::from_grams(grams))),
    ]
}

// Strategy helpers are free functions and may not panic; see
// `prescription_value_types.rs`.

fn reps() -> impl Strategy<Value = RepCount> {
    (1_u32..30).prop_filter_map("one and above is a rep count", |count| {
        RepCount::new(count).ok()
    })
}

fn effort() -> impl Strategy<Value = Rir> {
    prop_oneof![
        Just(Rir::Zero),
        Just(Rir::One),
        Just(Rir::Two),
        Just(Rir::Three),
    ]
}

fn target() -> impl Strategy<Value = Target<RepCount>> {
    prop_oneof![
        reps().prop_map(Target::Exactly),
        (1_u32..20, 1_u32..10).prop_filter_map("a spanning range of rep counts", |(low, span)| {
            Some(Target::spanning(
                RepCount::new(low).ok()?,
                RepCount::new(span).ok()?,
            ))
        }),
    ]
}

/// Every variant, so the property covers the whole partition rather than the one
/// arm generation happens to use most.
fn prescription() -> impl Strategy<Value = Prescribed<RepCount>> {
    prop_oneof![
        (load(), target(), proptest::option::of(effort())).prop_map(|(load, measure, effort)| {
            Prescribed::Fixed {
                load,
                measure,
                effort,
            }
        }),
        (load(), effort(), proptest::option::of(target())).prop_map(|(load, effort, predicted)| {
            Prescribed::ToEffort {
                load,
                effort,
                predicted,
            }
        }),
        (target(), effort())
            .prop_map(|(measure, effort)| Prescribed::Autoregulated { measure, effort }),
    ]
}

fn prescribed_set() -> impl Strategy<Value = PrescribedSet<RepCount>> {
    (prescription(), any::<bool>()).prop_map(|(prescription, warmup)| PrescribedSet {
        prescription,
        rest_after: None,
        warmup,
    })
}

fn reps_exercise() -> impl Strategy<Value = PrescribedExercise> {
    (
        prop_oneof![
            Just(RepsExercise::FrontSquat),
            Just(RepsExercise::PullUp),
            Just(RepsExercise::ChestDip),
        ],
        proptest::collection::vec(prescribed_set(), 1..5),
    )
        .prop_filter_map("one or more sets is a NonEmpty", |(exercise, sets)| {
            NonEmpty::new(sets)
                .ok()
                .map(|sets| PrescribedExercise::ForReps { exercise, sets })
        })
}

proptest! {
    /// § 24: every prescription pins at least one axis, so "prescribes nothing"
    /// is not a state any caller has to check for.
    #[test]
    fn an_arbitrary_prescription_pins_an_axis(prescription in prescription()) {
        let pins_load = prescription.load().is_some();
        let pins_measure = prescription.measure().is_some();
        let binds_effort = prescription.effort().is_some();
        prop_assert!(
            pins_load || pins_measure || binds_effort,
            "a prescription with no pinned axis instructs nothing"
        );
    }

    /// Each variant leaves exactly the axis it names open, which is what makes
    /// them three instructions rather than three spellings of one.
    #[test]
    fn each_variant_leaves_its_own_axis_open(prescription in prescription()) {
        match prescription {
            // Load and measure pinned; effort is guidance and may be absent.
            Prescribed::Fixed { .. } => {
                prop_assert!(prescription.load().is_some());
                prop_assert!(prescription.measure().is_some());
            }
            // Measure open. A prediction is not a prescription and must not
            // surface as one.
            Prescribed::ToEffort { .. } => {
                prop_assert!(prescription.load().is_some());
                prop_assert!(prescription.measure().is_none());
                prop_assert!(prescription.effort().is_some());
            }
            // Load open.
            Prescribed::Autoregulated { .. } => {
                prop_assert!(prescription.load().is_none());
                prop_assert!(prescription.measure().is_some());
                prop_assert!(prescription.effort().is_some());
            }
        }
    }

    /// An arbitrary exercise holds at least one set, and its warm-up count never
    /// exceeds its total.
    #[test]
    fn an_arbitrary_exercise_holds_sets(exercise in reps_exercise()) {
        prop_assert!(exercise.set_count() >= 1);
        prop_assert!(exercise.working_set_count() <= exercise.set_count());
    }

    /// An arbitrary shape has no empty exercise and no single-member superset,
    /// and every item reports at least one slot.
    #[test]
    fn an_arbitrary_shape_is_valid(
        first in reps_exercise(),
        second in reps_exercise(),
        third in reps_exercise(),
    ) {
        let superset = PrescribedSuperset {
            members: AtLeastTwo::of(
                SupersetMember { slot: SlotId::UpperPush, exercise: second },
                SupersetMember { slot: SlotId::UpperPull, exercise: third },
                Vec::new(),
            ),
        };
        let Ok(items) = NonEmpty::new(vec![
            PrescribedItem::Exercise { slot: SlotId::KneeDominant, exercise: first },
            PrescribedItem::Superset(superset),
        ]) else {
            panic!("two items were built")
        };
        let shape = WorkoutShape::new(items);

        prop_assert_eq!(shape.items().count(), 2);
        for item in shape.items().iter() {
            prop_assert!(item.slots().count() >= 1);
            prop_assert!(item.exercises().count() >= 1);
            if matches!(item, PrescribedItem::Superset(_)) {
                prop_assert!(item.exercises().count() >= 2);
            }
        }

        // Every exercise carries at least one set, so no set count is zero.
        for exercise in shape.exercises() {
            prop_assert!(exercise.set_count() >= 1);
        }
        prop_assert!(shape.set_count() >= 3);
    }
}

/// A superset may fill one slot with two members.
///
/// Not something the template issues — every superset it pairs is two distinct
/// slots — but what projection produces when a performed superset reaches a
/// position the template does not pair. The shape has to be able to hold it, and
/// `satisfies` reports the divergence.
#[test]
fn a_superset_may_fill_one_slot_twice() {
    let make = |exercise: RepsExercise| {
        let Ok(reps) = RepCount::new(6) else {
            panic!("six is a rep count")
        };
        let Ok(sets) = NonEmpty::new(vec![PrescribedSet::fixed(
            Load::BODYWEIGHT,
            Target::Exactly(reps),
        )]) else {
            panic!("one set was built")
        };
        PrescribedExercise::ForReps { exercise, sets }
    };

    let paired_into_one_slot = PrescribedSuperset {
        members: AtLeastTwo::of(
            SupersetMember {
                slot: SlotId::Core,
                exercise: make(RepsExercise::PreacherCurlBarbell),
            },
            SupersetMember {
                slot: SlotId::Core,
                exercise: make(RepsExercise::OverheadTricepsExtensionCable),
            },
            Vec::new(),
        ),
    };
    let item = PrescribedItem::Superset(paired_into_one_slot);

    let slots: Vec<SlotId> = item.slots().collect();
    assert_eq!(slots, vec![SlotId::Core, SlotId::Core]);
    assert_eq!(item.exercises().count(), 2);
}

/// A mobility slot pins its measure and no load, which is how open question 4
/// resolves: a duration *is* a pinned axis.
#[test]
fn a_mobility_slot_pins_its_duration() {
    let sixty = Duration::from_seconds(60);
    let set: PrescribedSet<Duration> = PrescribedSet::fixed(Load::UNLOADED, Target::Exactly(sixty));

    assert!(set.prescription.measure().is_some());
    assert_eq!(set.prescription.load(), Some(Load::UNLOADED));
    assert!(set.prescription.effort().is_none());

    let Ok(sets) = NonEmpty::new(vec![set]) else {
        panic!("one set was built")
    };
    let exercise = PrescribedExercise::ForDuration {
        exercise: DurationExercise::CouchStretch,
        sets,
    };
    assert_eq!(exercise.measure(), "duration");
}
