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
    plan::{MesocycleOrdinal, Plan, PlanName, Span},
    prescription::GymMesocycle,
    schedule::DayPart,
};

use crate::{
    commit::CommitError,
    error::StoreError,
    next::{MesocycleCommitter, StandingReader},
    ports::{DiaryStore, HolidayCalendar, PlanStore},
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

/// Everything reading the planned macrocycle reads.
pub struct PlanningPorts<W, C, D, H> {
    /// Where the microcycle walk stands, which says each mesocycle's state.
    pub walk: W,
    /// What builds the planned mesocycles after the written ones.
    pub committing: C,
    pub diary: D,
    /// The school calendar, which says what the term is.
    pub holidays: H,
}

/// The use case.
pub struct Planned<W, C, D, H> {
    ports: PlanningPorts<W, C, D, H>,
}

impl<W, C, D, H> Planned<W, C, D, H>
where
    W: StandingReader + Sync,
    C: MesocycleCommitter + Sync,
    D: DiaryStore + Sync,
    H: HolidayCalendar + Sync,
{
    pub const fn new(ports: PlanningPorts<W, C, D, H>) -> Self {
        Self { ports }
    }

    /// The macrocycle `now` falls in, as planned.
    ///
    /// `None` where the school calendar does not reach `now`, or no plan
    /// following a chain is in force this macrocycle.
    ///
    /// # Errors
    ///
    /// [`CommitError`] if the store or the school calendar cannot be read, or
    /// a planned mesocycle will not build.
    pub async fn on(&self, now: DayPart) -> Result<Option<PlannedMacrocycle>, CommitError> {
        let holidays = self.ports.holidays.holidays().await?;
        let standing = self.ports.walk.standing(now).await?;
        let diary = self.ports.diary.diary().await?;
        self.ports
            .committing
            .planned(now, &holidays, &standing, &diary)
            .await
    }
}

/// Every plan in the store, the latest version of each, oldest first.
pub(crate) async fn every<P: PlanStore + Sync>(store: &P) -> Result<Vec<Plan>, StoreError> {
    let mut plans = Vec::new();
    for window in store.windows().await? {
        if let Some(plan) = store.named(window.name()).await?
            && !plans.contains(&plan)
        {
            plans.push(plan);
        }
    }
    Ok(plans)
}
