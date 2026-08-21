//! The grid a derived load lands on, when the grid is not uniform.
//!
//! The barbell cases are the ones `quantise.rs` already pinned, restated
//! through the banded scale so the degenerate case is proved to be the old
//! behaviour rather than assumed to be. The dumbbell cases are the ones that
//! could not be expressed before, and one of them — 10kg leaving on a 2kg step
//! rather than the 1kg that reached it — is the whole reason the type changed.

use domain::{
    gym::Kg,
    prescription::{InvalidLoadSteps, LoadSteps, Step},
};

const fn kg(grams: u64) -> Kg {
    Kg::from_grams(grams)
}

/// The gym's barbell: 1.25kg a side, every load 2.5kg apart forever.
fn barbell() -> Result<LoadSteps, InvalidLoadSteps> {
    LoadSteps::uniform(kg(2_500))
}

/// The gym's rack: whole kilos to 10kg, twos above it.
fn dumbbell() -> Result<LoadSteps, InvalidLoadSteps> {
    LoadSteps::new(vec![
        Step {
            from: Kg::NONE,
            size: kg(1_000),
        },
        Step {
            from: kg(10_000),
            size: kg(2_000),
        },
    ])
}

#[test]
fn a_uniform_scale_rounds_to_the_nearest_step() {
    let steps = barbell().expect("a barbell is one band");
    // −10% of a failed 95kg, which is what opens a block.
    assert_eq!(steps.quantise(kg(85_500)), kg(85_000));
    // 85% of a 90kg top set, which is what a back-off is.
    assert_eq!(steps.quantise(kg(76_500)), kg(77_500));
    // −5% of 95kg, the reading of the entry drop that this block cannot
    // distinguish from the other by arithmetic alone.
    assert_eq!(steps.quantise(kg(90_250)), kg(90_000));
}

#[test]
fn an_exact_tie_resolves_down() {
    let steps = barbell().expect("a barbell is one band");
    // −10% from 87.5 is 78.75, exactly halfway between 77.5 and 80. This is the
    // case the single rounding rule was generalised for.
    assert_eq!(steps.quantise(kg(78_750)), kg(77_500));
}

#[test]
fn a_load_already_on_the_grid_is_left_alone() {
    let steps = barbell().expect("a barbell is one band");
    assert_eq!(steps.quantise(kg(90_000)), kg(90_000));
    let rack = dumbbell().expect("the rack has two bands");
    assert_eq!(rack.quantise(kg(7_000)), kg(7_000));
    assert_eq!(rack.quantise(kg(12_000)), kg(12_000));
}

#[test]
fn the_step_in_force_changes_with_the_load() {
    let steps = dumbbell().expect("the rack has two bands");
    assert_eq!(steps.step_at(kg(0)), kg(1_000));
    assert_eq!(steps.step_at(kg(9_000)), kg(1_000));
    // At the boundary the heavier band is already in force.
    assert_eq!(steps.step_at(kg(10_000)), kg(2_000));
    assert_eq!(steps.step_at(kg(24_000)), kg(2_000));
}

#[test]
fn double_progression_leaves_a_load_on_the_step_it_is_leaving() {
    let steps = dumbbell().expect("the rack has two bands");
    // 7kg is where the wrist extension sat, and 8kg is the next dumbbell up.
    // The old single increment made this 9.5kg, which is not a dumbbell.
    assert_eq!(steps.next_above(kg(7_000)), kg(8_000));
    // 10kg leaves on the 2kg that applies from 10kg, not the 1kg that got it
    // there. The old single increment made this 12.5kg.
    assert_eq!(steps.next_above(kg(10_000)), kg(12_000));
    assert_eq!(steps.next_above(kg(9_000)), kg(10_000));
}

#[test]
fn a_load_between_bands_rounds_to_a_weight_that_exists() {
    let steps = dumbbell().expect("the rack has two bands");
    // 9.6 sits in the 1kg band; 10 is the next real dumbbell and is nearer.
    assert_eq!(steps.quantise(kg(9_600)), kg(10_000));
    // 11 is equidistant between 10 and 12, so it resolves down.
    assert_eq!(steps.quantise(kg(11_000)), kg(10_000));
    assert_eq!(steps.quantise(kg(11_500)), kg(12_000));
}

#[test]
fn a_loaded_slot_is_never_prescribed_an_empty_bar() {
    let steps = barbell().expect("a barbell is one band");
    assert_eq!(steps.quantise(kg(1_000)), Kg::NONE);
    assert_eq!(steps.quantise_loaded(kg(1_000)), kg(2_500));
}

#[test]
fn a_scale_that_does_not_start_at_nothing_is_refused() {
    let refused = LoadSteps::new(vec![Step {
        from: kg(10_000),
        size: kg(2_000),
    }]);
    assert_eq!(refused, Err(InvalidLoadSteps::DoesNotStartAtZero));
}

#[test]
fn bands_out_of_order_are_refused_rather_than_sorted() {
    let refused = LoadSteps::new(vec![
        Step {
            from: Kg::NONE,
            size: kg(1_000),
        },
        Step {
            from: Kg::NONE,
            size: kg(2_000),
        },
    ]);
    assert_eq!(refused, Err(InvalidLoadSteps::NotAscending));
}

#[test]
fn a_step_of_nothing_is_refused() {
    assert_eq!(
        LoadSteps::uniform(Kg::NONE),
        Err(InvalidLoadSteps::StepOfNothing)
    );
}

#[test]
fn every_load_falls_inside_the_scale() {
    let steps = dumbbell().expect("the rack has two bands");
    // Total by construction: the first band starts at nothing, so there is no
    // load without a step and `step_at` needs no fallback.
    for grams in (0..40_000).step_by(250) {
        let quantised = steps.quantise(kg(grams));
        assert_eq!(
            steps.quantise(quantised),
            quantised,
            "quantising {grams}g was not idempotent"
        );
    }
}
