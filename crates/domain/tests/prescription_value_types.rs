//! § 28 over the prescribed vocabulary (T025).
//!
//! A randomly generated instance of a type must be valid. Where an arbitrary
//! instance can violate an invariant, the type is wrong rather than the
//! generator — so these strategies build through the real constructors and the
//! properties assert what those constructors are supposed to guarantee.

use domain::gym::{Kg, RepCount};
use domain::prescription::{
    Anchor, AnchorProvenance, PerRole, Percentage, PlateIncrement, SessionRole, SlotId, Target,
    TopSetReps, WeekIndex, WeekKind,
};
use jiff::civil::Date;
use proptest::prelude::*;

// Strategy helpers are free functions, so the test exemptions do not reach them
// and they may not panic. `prop_filter_map` is how the existing suites build
// through a fallible constructor without one.

fn percentage() -> impl Strategy<Value = Percentage> {
    // Non-zero, which is the invariant. Both signs, because a reset drop is a
    // negative percentage on the same axis as every other one.
    prop_oneof![-100_000_i32..=-1, 1_i32..=100_000]
        .prop_filter_map("non-zero basis points are a percentage", |points| {
            Percentage::from_basis_points(points).ok()
        })
}

fn a_date() -> impl Strategy<Value = Date> {
    (2020_i16..2030, 1_i8..=12, 1_i8..=28)
        .prop_filter_map("a valid calendar date", |(year, month, day)| {
            Date::new(year, month, day).ok()
        })
}

proptest! {
    /// Every percentage round-trips through its text form exactly.
    ///
    /// The point of holding basis points rather than a float: a stored
    /// prescription that cannot be reproduced is not a record of anything.
    #[test]
    fn a_percentage_round_trips_through_its_text_form(percentage in percentage()) {
        let rendered = percentage.to_string();
        let Ok(parsed) = Percentage::try_from(rendered.clone()) else {
            panic!("{rendered} was produced by Display and must parse")
        };
        prop_assert_eq!(parsed, percentage);
        prop_assert_eq!(parsed.as_basis_points(), percentage.as_basis_points());
    }

    /// A share of a mass never exceeds the mass unless the percentage does, and
    /// a negative share is no load rather than a negative one.
    #[test]
    fn a_share_of_a_mass_stays_within_the_axis(
        grams in 0_u64..500_000,
        percentage in percentage(),
    ) {
        let mass = Kg::from_grams(grams);
        let share = percentage.of(mass);

        if percentage.as_basis_points() < 0 {
            // There is no such thing as a negative amount of weight on a bar.
            prop_assert_eq!(share, Kg::NONE);
        } else if percentage <= Percentage::WHOLE {
            prop_assert!(share <= mass);
        }
    }

    /// A drop applied to a mass reduces it; a gain raises it. The distinction
    /// between `of` and `applied_to` is what keeps a reset landing on the right
    /// bar.
    #[test]
    fn applying_a_drop_reduces_and_a_gain_raises(grams in 10_000_u64..300_000) {
        let mass = Kg::from_grams(grams);
        let (Ok(drop), Ok(gain)) = (
            Percentage::try_from("-10%".to_owned()),
            Percentage::try_from("10%".to_owned()),
        ) else {
            panic!("both are percentages")
        };

        prop_assert!(drop.applied_to(mass) < mass);
        prop_assert!(gain.applied_to(mass) > mass);
        // And `of` is a different question: a tenth of the mass, not a tenth off.
        prop_assert!(gain.of(mass) < mass);
    }

    /// A plate increment is never zero, so nothing that divides by one can
    /// divide by nothing.
    #[test]
    fn an_increment_is_never_zero(grams in 1_u64..50_000) {
        let Ok(increment) = PlateIncrement::new(Kg::from_grams(grams)) else {
            panic!("a non-zero step is an increment")
        };
        prop_assert!(increment.as_kg().as_grams() > 0);
    }

    #[test]
    fn a_zero_increment_is_refused(_ in 0_u8..1) {
        prop_assert!(PlateIncrement::new(Kg::NONE).is_err());
    }

    /// An anchor always carries a load, its provenance and its date.
    #[test]
    fn an_arbitrary_anchor_is_valid(
        grams in 1_u64..400_000,
        provenance in prop_oneof![
            Just(AnchorProvenance::Tested),
            Just(AnchorProvenance::Estimated),
            Just(AnchorProvenance::Asserted),
        ],
        from in a_date(),
    ) {
        let Ok(anchor) = Anchor::new(Kg::from_grams(grams), None, provenance, from) else {
            panic!("a non-zero load is an anchor")
        };
        prop_assert!(anchor.load().as_grams() > 0);
        prop_assert_eq!(anchor.provenance(), provenance);
        prop_assert_eq!(anchor.from(), from);
    }

    #[test]
    fn an_anchor_of_no_load_is_refused(from in a_date()) {
        prop_assert!(Anchor::new(Kg::NONE, None, AnchorProvenance::Tested, from).is_err());
    }

    /// A failed load at or below what was completed is not a ceiling.
    ///
    /// It would open the block below the test that anchors it, which is not a
    /// state the model has: a test either found the ceiling above its best set
    /// or did not find one at all.
    #[test]
    fn a_failed_load_below_the_completed_one_is_refused(
        grams in 20_000_u64..300_000,
        under in 0_u64..20_000,
        from in a_date(),
    ) {
        let completed = Kg::from_grams(grams);
        prop_assert!(
            Anchor::new(
                completed,
                Some(Kg::from_grams(grams - under)),
                AnchorProvenance::Tested,
                from,
            )
            .is_err()
        );
        prop_assert!(
            Anchor::new(
                completed,
                Some(Kg::from_grams(grams + 2_500)),
                AnchorProvenance::Tested,
                from,
            )
            .is_ok()
        );
    }

    /// Weeks are one-based, and zero is refused rather than silently meaning the
    /// first week.
    #[test]
    fn a_week_index_is_one_based(week in 1_u32..500) {
        let Ok(index) = WeekIndex::new(week) else {
            panic!("one and above is a week")
        };
        prop_assert_eq!(index.as_u32(), week);
        prop_assert_eq!(index.as_offset(), week - 1);
    }

    #[test]
    fn week_zero_is_refused(_ in 0_u8..1) {
        prop_assert!(WeekIndex::new(0).is_err());
    }

    /// A range must span. Equal bounds are `Exactly` and there is no third state.
    #[test]
    fn a_range_never_holds_a_low_bound_at_or_above_its_high(low in 1_u32..100, high in 1_u32..100) {
        let (Ok(low_reps), Ok(high_reps)) = (RepCount::new(low), RepCount::new(high)) else {
            panic!("one and above is a rep count")
        };
        let built = Target::range(low_reps, high_reps);
        prop_assert_eq!(built.is_ok(), low < high);

        if let Ok(Target::Range { low: l, high: h }) = built {
            prop_assert!(l < h);
        }
    }

    /// Satisfaction is asymmetric, and that is a property of the domain: a
    /// performed count inside a prescribed range agrees with it.
    #[test]
    fn a_count_inside_a_range_satisfies_it(low in 1_u32..50, span in 1_u32..20, offset in 0_u32..30) {
        let high = low + span;
        let (Ok(low_reps), Ok(high_reps)) = (RepCount::new(low), RepCount::new(high)) else {
            panic!("one and above is a rep count")
        };
        let Ok(target) = Target::range(low_reps, high_reps) else {
            panic!("a positive span is a range")
        };
        let Ok(performed) = RepCount::new(low + offset) else {
            panic!("one and above is a rep count")
        };

        prop_assert_eq!(target.satisfied_by(&performed), offset <= span);
    }

    /// `PerRole` always answers for both roles, which is why it is a struct.
    #[test]
    fn per_role_answers_for_both_roles(light in 1_u32..20, heavy in 1_u32..20) {
        let (Ok(light_reps), Ok(heavy_reps)) = (RepCount::new(light), RepCount::new(heavy)) else {
            panic!("one and above is a rep count")
        };
        let per_role = PerRole {
            light: TopSetReps::new(light_reps),
            heavy: TopSetReps::new(heavy_reps),
        };
        for role in SessionRole::ALL {
            let reps = per_role.get(*role);
            prop_assert!(reps.as_rep_count().as_u32() > 0);
        }
    }
}

/// Every slot's key is distinct, and every key reads back.
///
/// A table test rather than a property because the vocabulary is closed and
/// enumerating it is the whole assertion.
#[test]
fn every_slot_key_is_distinct_and_reads_back() {
    let mut keys: Vec<&str> = SlotId::ALL.iter().map(|slot| slot.as_str()).collect();
    let before = keys.len();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), before, "slot keys must be distinct");
    assert_eq!(before, 17, "the template has seventeen slots");

    for slot in SlotId::ALL {
        let Ok(parsed) = SlotId::try_from(slot.as_str().to_owned()) else {
            panic!("{slot} is its own key")
        };
        assert_eq!(parsed, *slot);
    }
}

#[test]
fn a_test_week_is_not_a_ladder_position() {
    assert_eq!(WeekKind::Test.index(), None);
    let Ok(week) = WeekIndex::new(3) else {
        panic!("three is a week")
    };
    assert_eq!(WeekKind::Climbing(week).index(), Some(week));
}

#[test]
fn session_roles_and_provenances_read_back() {
    for role in SessionRole::ALL {
        let Ok(parsed) = SessionRole::try_from(role.as_str().to_owned()) else {
            panic!("{role} is its own key")
        };
        assert_eq!(parsed, *role);
    }
    for provenance in [
        AnchorProvenance::Tested,
        AnchorProvenance::Estimated,
        AnchorProvenance::Asserted,
    ] {
        let Ok(parsed) = AnchorProvenance::try_from(provenance.as_str().to_owned()) else {
            panic!("{provenance} is its own key")
        };
        assert_eq!(parsed, provenance);
    }
}
