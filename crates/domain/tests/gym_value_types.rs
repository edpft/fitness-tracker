//! § 28: a randomly generated instance of a type must be valid.
//!
//! If an arbitrary instance can violate an invariant, the type is wrong — not
//! the generator. So every generator here builds through the public
//! constructor and the property is that what comes out is usable, never that a
//! hand-picked example works.

use domain::gym::{
    Load, Performed, Rir, SetKind, SignedKg,
    exercise::{DistanceExercise, DurationExercise, RepsExercise},
};
use domain::measure::{Distance, Kg, Metres, RepCount};
use domain::sequence::{AtLeastTwo, NonEmpty};
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

    /// An absolute load is external load, and zero is a real answer: a
    /// bodyweight squat, a set of skipping. What it can never be is negative,
    /// which the type carries rather than checks.
    #[test]
    fn an_absolute_load_is_external_load_and_admits_none(grams in 0_u64..500_000) {
        let Load::Absolute(held) = Load::absolute(Kg::from_grams(grams)) else {
            prop_assert!(false, "absolute built a relative load");
            return Ok(());
        };
        prop_assert_eq!(held.as_grams(), grams);
        prop_assert_eq!(held.is_none(), grams == 0);
    }

    /// A pound reading converts exactly, without a float and without a second
    /// rounding step.
    #[test]
    fn a_pound_reading_converts_without_a_float(pounds in 0_u32..1_000) {
        let text = pounds.to_string();
        let Ok(mass) = Kg::from_pounds(&text) else {
            prop_assert!(false, "{text} lb is a mass");
            return Ok(());
        };
        prop_assert_eq!(mass.as_grams(), u64::from(pounds) * 453_592 / 1_000);
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
    fn a_distance_round_trips(millimetres in 0_u64..1_000_000) {
        let metres = Metres::from_millimetres(millimetres);
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

    assert_eq!(
        seen.len(),
        186,
        "the vocabulary this build has needed so far: 136, 14 from the spreadsheets (#274), \
         the suitcase carry from `CT 2017`'s earlier copies (#280), 25 from Beyond The \
         White Board (#285), the triceps pushdown and triceps dip the record needed \
         and no source could name (#300), and eight more the operator named on 2026-09-29 so \
         the watch's own terms had somewhere to land (#305)"
    );
}

/// Every movement's key is distinct and reversible, and every one of them is a
/// movement some exercise is.
///
/// The second half is what keeps the grouping honest. A movement with no member
/// is a word nothing in the record answers to, and the only way to add one is to
/// declare it beside a key — so this fails the moment a name is invented for its
/// own sake rather than for an exercise that needed it.
#[test]
fn every_movement_is_distinct_reversible_and_performed() {
    use domain::gym::exercise::{DistanceExercise, DurationExercise, Movement, RepsExercise};

    let mut keys = std::collections::BTreeSet::new();
    for &movement in Movement::ALL {
        let key = movement.as_str();
        assert!(keys.insert(key), "{key} names two movements");
        assert_eq!(Ok(movement), Movement::try_from(key));
    }
    assert_eq!(keys.len(), 84, "the movements the operator settled on #305");

    let mut declared = std::collections::BTreeSet::new();
    macro_rules! collect {
        ($vocabulary:ty) => {
            for &exercise in <$vocabulary>::ALL {
                declared.insert(exercise.movement());
            }
        };
    }
    collect!(RepsExercise);
    collect!(DurationExercise);
    collect!(DistanceExercise);

    let unperformed: Vec<&str> = Movement::ALL
        .iter()
        .filter(|movement| !declared.contains(movement))
        .map(|movement| movement.as_str())
        .collect();
    assert!(
        unperformed.is_empty(),
        "no exercise is one of: {unperformed:?}"
    );
}

/// A held position worked one side at a time is held twice.
///
/// The couch stretch and the 90/90 are the two the operator named, and the
/// corpus agrees without being asked: every one of the 14 entries for each is
/// two sets, while every dead hang is one. The pigeon joins them on the same
/// grounds — a hip belongs to one leg — and has no corpus to agree, having
/// never been performed. Everything else opens both sides at once: a squatting
/// groin stretch and a standing straddle fold included.
///
/// Total by construction, since `sides` is an exhaustive match: a twelfth
/// duration exercise will not compile until someone says which it is.
#[test]
fn a_position_held_one_side_at_a_time_is_held_twice() {
    use domain::gym::{Sides, exercise::DurationExercise};

    for exercise in DurationExercise::ALL {
        let expected = match exercise {
            DurationExercise::CouchStretch
            | DurationExercise::NinetyNinety
            | DurationExercise::PigeonStretch
            | DurationExercise::SuitcaseHold => Sides::Separately,
            _ => Sides::Together,
        };
        assert_eq!(exercise.sides(), expected, "{exercise}");
    }

    assert_eq!(Sides::Separately.holds(), 2, "a body has two sides");
    assert_eq!(Sides::Together.holds(), 1);
}

/// A set carries its measure in its type, so these four are the only shapes
/// that exist and none of them can be built with the wrong one. The test is
/// that the code below compiles at all; the assertions are incidental.
#[test]
fn a_set_cannot_disagree_with_its_exercise() {
    let Ok(reps) = RepCount::new(5) else {
        panic!("five is a rep count")
    };
    let metres = Metres::from_millimetres(20_000);

    let for_reps = domain::gym::Set {
        load: Load::BODYWEIGHT,
        outcome: Performed::Completed(reps),
        intensity: Some(Rir::Two),
        kind: SetKind::Working,
        rest_after: None,
    };
    let for_distance = domain::gym::Set {
        load: Load::BODYWEIGHT,
        outcome: Performed::Completed(Distance { metres }),
        intensity: None,
        kind: SetKind::Warmup,
        rest_after: None,
    };

    let Some(reps_performed) = for_reps.outcome.completed() else {
        panic!("a completed set carries its measure")
    };
    let Some(distance_performed) = for_distance.outcome.completed() else {
        panic!("a completed set carries its measure")
    };
    assert_eq!(reps_performed.as_u32(), 5);
    assert_eq!(distance_performed.metres, metres);
}

/// Heavier than, within one axis and never across two (issue #129).
///
/// The relative case is the one the axis exists for: assistance and added weight
/// are one line through zero, so less assistance is heavier and a weighted rep is
/// heavier still.
#[test]
fn a_load_is_ordered_within_its_axis_and_not_across_them() {
    let assisted = Load::relative(SignedKg::from_grams(-20_000));
    let barely = Load::relative(SignedKg::from_grams(-5_000));
    let bodyweight = Load::BODYWEIGHT;
    let weighted = Load::relative(SignedKg::from_grams(10_000));

    assert!(assisted < barely, "less assistance is heavier");
    assert!(barely < bodyweight, "no assistance is heavier still");
    assert!(
        bodyweight < weighted,
        "and added weight is heavier than that"
    );

    let light = Load::absolute(Kg::from_grams(20_000));
    let heavy = Load::absolute(Kg::from_grams(60_000));
    assert!(light < heavy);

    // A bodyweight squat and a plain bodyweight pull-up are both "no external
    // load" and neither is heavier. There is no total order to reach for.
    assert_eq!(Load::UNLOADED.partial_cmp(&Load::BODYWEIGHT), None);
    assert_eq!(heavy.partial_cmp(&assisted), None);
}
