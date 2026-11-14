//! What the operator is eating for (#489).
//!
//! **The phase is assumed from the macrocycle** (#258): a cut for the whole
//! term, maintenance in the transition (operator, 2026-11-11). He sets the
//! goal up by hand in the platform that logs his calories, and nothing here
//! reads it. The competitive phase was maintenance from 2026-10-19 only in
//! the hope of reaching his target before it.

use crate::macrocycle::Phase;

/// What the operator is eating for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NutritionPhase {
    Cut,
    Maintenance,
    Bulk,
}

impl NutritionPhase {
    /// The phase assumed for a macrocycle's phase.
    pub const fn assumed(phase: Phase) -> Self {
        match phase {
            Phase::Preparatory | Phase::Competitive => Self::Cut,
            Phase::Transition => Self::Maintenance,
        }
    }
}
