//! The ramp's repetition counts follow the top set's (decision 0030).

use domain::{
    gym::{RepCount, sequence::NonEmpty},
    prescription::{Percentage, WarmupStep, warmup::ramp},
};

/// The operator's own ramp, which decision 0030 makes the floor: 4 at 40%, 3 at
/// 60%, 2 at 80%, 1 at 90%.
fn floor() -> Result<NonEmpty<WarmupStep>, Box<dyn std::error::Error>> {
    let mut steps = Vec::with_capacity(4);
    for (points, reps) in [(4_000, 4), (6_000, 3), (8_000, 2), (9_000, 1)] {
        steps.push(WarmupStep {
            of_top_set: Percentage::from_basis_points(points)?,
            reps: RepCount::new(reps)?,
        });
    }
    Ok(NonEmpty::new(steps)?)
}

fn reps_of(steps: &NonEmpty<WarmupStep>) -> Vec<u32> {
    steps.iter().map(|step| step.reps.as_u32()).collect()
}

fn ramp_for(top: u32) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    Ok(reps_of(&ramp(&floor()?, RepCount::new(top)?)))
}

#[test]
fn the_published_five_rep_shape_is_what_everett_states() {
    // "1-2 sets of 5, then sets of 3 and 2 until getting to your 5 rep weight."
    // Greg Everett, Catalyst Athletics, 2017-11-13.
    let ramp = ramp_for(5).expect("the floor and a five-rep top set both build");
    assert_eq!(ramp, vec![5, 5, 3, 2]);
}

#[test]
fn the_eight_rep_shape_is_the_operators() {
    // "I'd say the 8rm shape should be more like 8,8,6,5" — 2026-09-03. Stated
    // independently of Everett's five, and the same rule.
    let ramp = ramp_for(8).expect("the floor and an eight-rep top set both build");
    assert_eq!(ramp, vec![8, 8, 6, 5]);
}

#[test]
fn a_low_top_set_leaves_the_stored_ramp_alone() {
    // The descent would reach zero at three and go negative at one, so the floor
    // governs — which is why the 3RM and 1RM days are unchanged by 0030.
    for top in [1, 2, 3] {
        let ramp = ramp_for(top).expect("the floor and a low top set both build");
        assert_eq!(ramp, vec![4, 3, 2, 1], "top set of {top}");
    }
}

#[test]
fn the_descent_is_at_or_above_the_floor_from_four_up() {
    // The property that makes an element-wise maximum and a branch on `n` the
    // same rule. If this ever fails, the two forms have come apart and the
    // module doc is wrong.
    for top in 4..=20 {
        let ramp = ramp_for(top).expect("the floor and this top set both build");
        let descent: Vec<u32> = (0..4)
            .map(|index| if index < 2 { top } else { top - index })
            .collect();
        assert_eq!(ramp, descent, "top set of {top}");
    }
}

#[test]
fn the_percentages_are_never_touched() {
    let floor = floor().expect("the floor builds");
    let ramped = ramp(
        &floor,
        RepCount::new(8).expect("eight is a repetition count"),
    );
    let before: Vec<i32> = floor
        .iter()
        .map(|step| step.of_top_set.as_basis_points())
        .collect();
    let after: Vec<i32> = ramped
        .iter()
        .map(|step| step.of_top_set.as_basis_points())
        .collect();
    assert_eq!(before, after);
}

#[test]
fn the_ramp_keeps_its_length() {
    for top in [1, 5, 8, 12] {
        let ramp = ramp_for(top).expect("the floor and this top set both build");
        assert_eq!(ramp.len(), 4, "top set of {top}");
    }
}
