//! The macrocycle as planned: the whole term, each mesocycle planned, in
//! progress or completed (#789). Plan opens with it (#770).
//!
//! **One picture of the macrocycle, read by every view.** Until #789 each view
//! built its own: Today's curves from a projection, Progress and Plan from the
//! stored mesocycles alone, so Plan's card stopped where the written plan did
//! rather than at the end of term.

use domain::{
    cycling::CyclingProvenance,
    macrocycle::{MesocycleState, PlannedMacrocycle},
    plan::{MesocycleOrdinal, PlanName, Span},
    prescription::GymMesocycle,
};

/// The planned macrocycle, mesocycle by mesocycle, as Plan lays it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macrocycle {
    pub name: PlanName,
    /// Oldest first, to the end of term.
    pub mesocycles: Vec<Mesocycle>,
}

/// One mesocycle: both disciplines' halves of it, either of which may be
/// missing while plans are authored a discipline at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mesocycle {
    pub ordinal: MesocycleOrdinal,
    pub span: Span,
    pub gym: Option<GymMesocycle>,
    pub cycling: Option<CyclingProvenance>,
    pub state: MesocycleState,
}

impl From<&PlannedMacrocycle> for Macrocycle {
    fn from(planned: &PlannedMacrocycle) -> Self {
        Self {
            name: planned.plan().name().clone(),
            mesocycles: planned
                .mesocycles()
                .into_iter()
                .map(|planned| {
                    let mesocycle = planned.mesocycle();
                    Mesocycle {
                        ordinal: planned.ordinal(),
                        span: planned.span(),
                        gym: mesocycle.gym().cloned(),
                        cycling: mesocycle
                            .cycling()
                            .map(|cycling| cycling.provenance().clone()),
                        state: planned.state(),
                    }
                })
                .collect(),
        }
    }
}
