//! Putting a derived load on the bar (T026).
//!
//! The rule is nearest multiple of the increment, ties resolving down. It is one
//! function serving three derivations — back-offs, warm-up steps and reset drops
//! — so getting it wrong is wrong in three places at once.

use domain::gym::Kg;
use domain::prescription::{PlateIncrement, quantise, quantise_loaded};
use proptest::prelude::*;

/// The 2.5kg grid every worked example in the model of record assumes.
fn plate_grid() -> Result<PlateIncrement, Box<dyn std::error::Error>> {
    let step = Kg::try_from("2.5".to_owned())?;
    Ok(PlateIncrement::new(step)?)
}

fn kg(value: &str) -> Result<Kg, Box<dyn std::error::Error>> {
    Ok(Kg::try_from(value.to_owned())?)
}

#[test]
fn the_named_cases_from_the_model_of_record() {
    let Ok(grid) = plate_grid() else {
        panic!("2.5kg is a plate increment")
    };

    // Each of these appears in the design documents by name, and each comes
    // from a different derivation: a back-off, a reset drop, and two back-offs
    // that the record already validated.
    let cases = [
        // 85% of an 80kg top set. The case that raised the question.
        ("68", "67.5"),
        // A -10% reset drop from 87.5. An exact tie, and the reason the rule
        // had to be stated rather than left to a nearest-integer default.
        ("78.75", "77.5"),
        // 85% of 87.5, from the session of 2026-08-14.
        ("74.375", "75"),
        // 85% of 85, from 2026-08-07.
        ("72.25", "72.5"),
    ];

    for (input, expected) in cases {
        let (Ok(input_kg), Ok(expected_kg)) = (kg(input), kg(expected)) else {
            panic!("{input} and {expected} are both masses")
        };
        assert_eq!(
            quantise(input_kg, grid),
            expected_kg,
            "{input} should quantise to {expected}"
        );
    }
}

#[test]
fn an_exact_tie_resolves_down() {
    let Ok(grid) = plate_grid() else {
        panic!("2.5kg is a plate increment")
    };
    // Exactly halfway between 67.5 and 70.
    let Ok(halfway) = kg("68.75") else {
        panic!("68.75 is a mass")
    };
    let Ok(lower) = kg("67.5") else {
        panic!("67.5 is a mass")
    };
    assert_eq!(quantise(halfway, grid), lower);
}

#[test]
fn a_loaded_slot_never_prescribes_an_empty_bar() {
    let Ok(grid) = plate_grid() else {
        panic!("2.5kg is a plate increment")
    };
    let Ok(tiny) = kg("0.4") else {
        panic!("0.4 is a mass")
    };

    // Plain quantisation says zero, which is a real answer for unloaded work.
    assert_eq!(quantise(tiny, grid), Kg::NONE);
    // The loaded form says one increment, because an empty barbell is not a
    // prescription anybody can follow.
    assert_eq!(quantise_loaded(tiny, grid), grid.as_kg());
}

proptest! {
    /// The three properties that define the rule, over arbitrary loads and
    /// arbitrary grids — including grids no gym has, because the rule should not
    /// depend on 2.5kg being the increment.
    #[test]
    fn quantising_lands_on_the_grid_within_half_a_step(
        grams in 0_u64..500_000,
        step_grams in 1_u64..20_000,
    ) {
        let load = Kg::from_grams(grams);
        let Ok(increment) = PlateIncrement::new(Kg::from_grams(step_grams)) else {
            panic!("a non-zero step is an increment")
        };

        let result = quantise(load, increment);

        // On the grid.
        prop_assert_eq!(result.as_grams() % step_grams, 0);

        // Within half a step. `<=` rather than `<`, because an exact tie is
        // exactly half a step away and is a legal answer.
        let distance = result.as_grams().abs_diff(grams);
        prop_assert!(distance * 2 <= step_grams);

        // And the lower of two equidistant candidates: if it rounded up, the
        // gap must have been strictly more than half.
        if result.as_grams() > grams {
            prop_assert!(distance * 2 < step_grams);
        }
    }

    /// Quantising something already on the grid changes nothing. A separate
    /// property because it is the case a naive implementation gets wrong by
    /// nudging every value up one step.
    #[test]
    fn a_load_already_on_the_grid_is_unchanged(
        multiple in 0_u64..2_000,
        step_grams in 1_u64..20_000,
    ) {
        let Ok(increment) = PlateIncrement::new(Kg::from_grams(step_grams)) else {
            panic!("a non-zero step is an increment")
        };
        let load = Kg::from_grams(multiple * step_grams);
        prop_assert_eq!(quantise(load, increment), load);
    }
}
