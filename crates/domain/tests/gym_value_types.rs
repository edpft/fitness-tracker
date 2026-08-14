//! § 28: a randomly generated instance of a type must be valid.
//!
//! If an arbitrary instance can violate an invariant, the type is wrong — not
//! the generator. So every generator here builds through the public
//! constructor and the property is that what comes out is usable, never that a
//! hand-picked example works.

use domain::gym::{
    Distance, Duration, Kg, Load, Metres, NonEmpty, RepCount, Rir, SetKind, SignedKg,
    TimedDistance,
    exercise::{DistanceExercise, DurationExercise, RepsExercise, TimedDistanceExercise},
    nonempty::AtLeastTwo,
};
use proptest::prelude::*;

/// A load's text form is what gets persisted and compared against rows written
/// by earlier versions (§ 7), so it has to survive the round trip exactly. This
/// is the property a float would fail: `20.4` parsed as an `f64` and rendered
/// back is not reliably `20.4`.
fn kilograms() -> impl Strategy<Value = String> {
    (0_u32..500, 0_u32..1000).prop_map(|(whole, thousandths)| format!("{whole}.{thousandths:03}"))
}

proptest! {
    #[test]
    fn a_mass_round_trips_through_its_text_form(text in kilograms()) {
        let Ok(mass) = Kg::try_from(text.as_str()) else {
            prop_assert!(false, "{text} is a valid mass");
            return Ok(());
        };
        let reparsed = Kg::try_from(mass.to_string().as_str());
        prop_assert_eq!(Ok(mass), reparsed);
    }

    #[test]
    fn a_signed_mass_round_trips_through_its_text_form(text in kilograms(), negative: bool) {
        let text = if negative { format!("-{text}") } else { text };
        let Ok(delta) = SignedKg::try_from(text.as_str()) else {
            prop_assert!(false, "{text} is a valid signed mass");
            return Ok(());
        };
        prop_assert_eq!(Ok(delta), SignedKg::try_from(delta.to_string().as_str()));
    }

    /// Trailing zeros are not information. `20.4`, `20.40` and `20.400` are one
    /// load, which matters because a digest over a rendered value must not
    /// depend on how the source spelled it.
    #[test]
    fn trailing_zeros_do_not_change_a_mass(whole in 0_u32..500, tenths in 0_u32..10) {
        let short = Kg::try_from(format!("{whole}.{tenths}").as_str());
        let long = Kg::try_from(format!("{whole}.{tenths}00").as_str());
        prop_assert_eq!(short, long);
    }

    /// More precision than a gram is refused rather than rounded. Rounding here
    /// would be a silent edit to an observation.
    #[test]
    fn a_mass_finer_than_a_gram_is_refused(whole in 0_u32..500, fraction in 1000_u32..10_000) {
        let text = format!("{whole}.{fraction}");
        prop_assert!(Kg::try_from(text.as_str()).is_err());
    }

    /// The one thing `Load::Absolute` exists to make impossible.
    #[test]
    fn an_absolute_load_is_never_zero(grams in 0_i64..500_000) {
        let Some(mass) = Kg::from_grams(grams) else {
            prop_assert!(false, "a non-negative gram count is a mass");
            return Ok(());
        };
        match Load::absolute(mass) {
            Ok(Load::Absolute(held)) => prop_assert!(!held.is_zero()),
            Ok(Load::Relative(_)) => prop_assert!(false, "absolute built a relative load"),
            Err(_) => prop_assert_eq!(grams, 0),
        }
    }

    /// Zero is a real observation on the relative axis — it is a plain
    /// bodyweight pull-up, not an absence — and negative is assistance.
    #[test]
    fn a_relative_load_admits_zero_and_negatives(grams in -100_000_i64..500_000) {
        let Load::Relative(delta) = Load::relative(SignedKg::from_grams(grams)) else {
            prop_assert!(false, "relative built an absolute load");
            return Ok(());
        };
        prop_assert_eq!(delta.as_grams(), grams);
    }

    /// Assistance and added weight are one axis, and the crossover through zero
    /// must not change type.
    #[test]
    fn negation_is_its_own_inverse(grams in -500_000_i64..500_000) {
        let delta = SignedKg::from_grams(grams);
        prop_assert_eq!(delta.negated().negated(), delta);
    }

    /// A set of zero reps is an attempt, not a set.
    #[test]
    fn a_rep_count_is_never_zero(reps in 0_u32..1000) {
        if let Ok(count) = RepCount::new(reps) { prop_assert_eq!(count.as_u32(), reps) } else { prop_assert_eq!(reps, 0) }
    }

    #[test]
    fn a_distance_round_trips(millimetres in 0_i64..1_000_000) {
        let Some(metres) = Metres::from_millimetres(millimetres) else {
            prop_assert!(false, "a non-negative millimetre count is a distance");
            return Ok(());
        };
        prop_assert_eq!(Ok(metres), Metres::try_from(metres.to_string().trim_end_matches('m')));
    }

    /// The scale orders and compares. It does not average or subtract, and the
    /// type offers no way to try.
    #[test]
    fn intensity_orders_without_arithmetic(a in 0_usize..8, b in 0_usize..8) {
        let (Some(&first), Some(&second)) = (Rir::ALL.get(a), Rir::ALL.get(b)) else {
            prop_assert!(false, "the scale has eight positions");
            return Ok(());
        };
        prop_assert_eq!(first < second, a < b);
        prop_assert_eq!(Ok(first), Rir::try_from(first.as_str()));
    }

    /// A non-empty sequence always has a first element to hand back, and that
    /// is a fact about its shape rather than a promise its constructor made.
    #[test]
    fn a_non_empty_sequence_always_has_a_first(head: u8, tail: Vec<u8>) {
        let expected = tail.len().saturating_add(1);
        let sequence = NonEmpty::of(head, tail);
        prop_assert_eq!(*sequence.first(), head);
        prop_assert_eq!(sequence.count(), expected);
        prop_assert_eq!(sequence.iter().count(), expected);
    }

    /// Fewer than two is not a superset, and the constructor is the only way in.
    #[test]
    fn two_or_more_rejects_anything_shorter(items: Vec<u8>) {
        let expected = items.len();
        match AtLeastTwo::new(items) {
            Ok(group) => {
                prop_assert!(expected >= 2);
                prop_assert_eq!(group.count(), expected);
                prop_assert_eq!(group.iter().count(), expected);
            }
            Err(_) => prop_assert!(expected < 2),
        }
    }
}

/// Every vocabulary's keys are distinct and read back to the variant that wrote
/// them. A collision would silently merge two exercises into one series, which
/// is the failure the whole mapping exists to prevent.
#[test]
fn every_exercise_key_is_distinct_and_reversible() {
    let mut seen = std::collections::BTreeSet::new();

    macro_rules! check {
        ($vocabulary:ty) => {
            for &exercise in <$vocabulary>::ALL {
                let key = exercise.as_str();
                assert!(seen.insert(key), "{key} names two exercises");
                assert_eq!(Ok(exercise), <$vocabulary>::try_from(key));
            }
        };
    }

    check!(RepsExercise);
    check!(DurationExercise);
    check!(DistanceExercise);
    check!(TimedDistanceExercise);

    assert_eq!(seen.len(), 130, "the vocabulary the corpus needs");
}

/// A set carries its measure in its type, so these four are the only shapes
/// that exist and none of them can be built with the wrong one. The test is
/// that the code below compiles at all; the assertions are incidental.
#[test]
fn a_set_cannot_disagree_with_its_exercise() {
    let Ok(reps) = RepCount::new(5) else {
        panic!("five is a rep count")
    };
    let Some(metres) = Metres::from_millimetres(20_000) else {
        panic!("twenty metres is a distance")
    };

    let for_reps = domain::gym::Set {
        load: Load::BODYWEIGHT,
        measure: reps,
        intensity: Some(Rir::Two),
        kind: SetKind::Working,
        rest_after: None,
    };
    let for_distance = domain::gym::Set {
        load: Load::BODYWEIGHT,
        measure: Distance { metres },
        intensity: None,
        kind: SetKind::Warmup,
        rest_after: None,
    };
    let for_timed = domain::gym::Set {
        load: Load::BODYWEIGHT,
        measure: TimedDistance {
            metres,
            duration: Duration::from_seconds(60),
        },
        intensity: None,
        kind: SetKind::Working,
        rest_after: None,
    };

    assert_eq!(for_reps.measure.as_u32(), 5);
    assert_eq!(for_distance.measure.metres, metres);
    assert_eq!(for_timed.measure.duration.as_seconds(), 60);
}
