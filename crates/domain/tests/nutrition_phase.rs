//! What the operator is eating for, assumed from the macrocycle (#258).

use domain::{analytical::nutrition_phase::NutritionPhase, macrocycle::Phase};

#[test]
fn the_term_is_a_cut_and_the_transition_maintenance() {
    assert_eq!(
        NutritionPhase::assumed(Phase::Preparatory),
        NutritionPhase::Cut
    );
    assert_eq!(
        NutritionPhase::assumed(Phase::Competitive),
        NutritionPhase::Cut
    );
    assert_eq!(
        NutritionPhase::assumed(Phase::Transition),
        NutritionPhase::Maintenance
    );
}
