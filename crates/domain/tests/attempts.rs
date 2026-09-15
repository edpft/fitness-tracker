//! A test's attempts (#136): three, a step apart, the last the target.

use domain::{
    gym::Kg,
    prescription::{Attempts, LoadSteps, Step},
};

const fn kg(grams: u64) -> Kg {
    Kg::from_grams(grams)
}

/// The autumn entry test, on the operator's own numbers.
#[test]
fn a_95kg_test_on_the_bar_is_attempted_at_90_then_92_5_then_95() {
    let bar = LoadSteps::uniform(kg(2_500)).expect("a barbell is one band");
    let attempts = Attempts::toward(kg(95_000), &bar);
    assert_eq!(attempts.loads(), [kg(90_000), kg(92_500), kg(95_000)]);
    assert_eq!(attempts.first(), kg(90_000), "the ramp is built off this");
    assert_eq!(
        attempts.target(),
        kg(95_000),
        "and nothing is prescribed past this"
    );
}

/// The step is the lift's own, and is read at each load being left.
#[test]
fn attempts_across_a_band_boundary_step_by_the_band_each_is_in() {
    let rack = LoadSteps::new(vec![
        Step {
            from: Kg::NONE,
            size: kg(1_000),
        },
        Step {
            from: kg(10_000),
            size: kg(2_000),
        },
    ])
    .expect("the rack has two bands");
    let attempts = Attempts::toward(kg(12_000), &rack);
    assert_eq!(attempts.loads(), [kg(9_000), kg(10_000), kg(12_000)]);
}
