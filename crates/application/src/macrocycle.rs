//! The macrocycle in force, as Plan opens with it (#770).
//!
//! The operator, 2026-11-10 (#749): the pages under Plan *"are all inputs to
//! planning but not planning themselves"*, and Plan should open with the plan.
//! So this is the plan in force today: its name, and each mesocycle with its
//! days, what each discipline runs in it, and which one is running.

use domain::{
    cycling::CyclingProvenance,
    plan::{MesocycleOrdinal, Plan, PlanName, Span},
    prescription::GymMesocycle,
};
use jiff::civil::Date;

use crate::{error::StoreError, ports::PlanStore};

/// The plan in force, mesocycle by mesocycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Macrocycle {
    pub name: PlanName,
    /// Oldest first.
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
    /// Whether it covers today.
    pub current: bool,
}

/// The use case.
pub struct InForce<P> {
    plans: P,
}

impl<P: PlanStore + Sync> InForce<P> {
    pub const fn new(plans: P) -> Self {
        Self { plans }
    }

    /// The macrocycle in force on `today`: `None` on a day no plan covers.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn on(&self, today: Date) -> Result<Option<Macrocycle>, StoreError> {
        Ok(every(&self.plans)
            .await?
            .iter()
            .find(|plan| plan.span().covers(today))
            .map(|plan| laid_out(plan, today)))
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

fn laid_out(plan: &Plan, today: Date) -> Macrocycle {
    let mesocycles = plan
        .mesocycle_spans()
        .into_iter()
        .filter_map(|span| plan.mesocycle_on(span.start()))
        .zip(1..)
        .filter_map(|(mesocycle, number)| {
            Some(Mesocycle {
                ordinal: MesocycleOrdinal::new(number).ok()?,
                span: mesocycle.span(),
                gym: mesocycle.gym().cloned(),
                cycling: mesocycle
                    .cycling()
                    .map(|cycling| cycling.provenance().clone()),
                current: mesocycle.span().covers(today),
            })
        })
        .collect();
    Macrocycle {
        name: plan.name().clone(),
        mesocycles,
    }
}
