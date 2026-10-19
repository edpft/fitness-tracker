//! The ranges a chart can be read over, on a day (#252, #497).
//!
//! **Worked out once, here, for every chart**: the mesocycle and the
//! macrocycle in force are the plans', and which days they start on is
//! `domain`'s ([`Ranges::on`]). Nothing is written.

use domain::analytical::range::Ranges;
use jiff::civil::Date;

use crate::{error::StoreError, ports::PlanStore};

/// The use case.
pub struct CurrentRanges<P> {
    plans: P,
}

impl<P> CurrentRanges<P>
where
    P: PlanStore + Sync,
{
    pub const fn new(plans: P) -> Self {
        Self { plans }
    }

    /// The mesocycle and macrocycle `today` falls in, each to `today`.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn on(&self, today: Date) -> Result<Ranges, StoreError> {
        let mut mesocycles = Vec::new();
        let mut plans = Vec::new();
        for window in self.plans.windows().await? {
            if let Some(plan) = self.plans.named(window.name()).await? {
                mesocycles.extend(plan.mesocycle_spans());
                plans.push(plan.span());
            }
        }
        Ok(Ranges::on(today, &mesocycles, &plans))
    }
}
