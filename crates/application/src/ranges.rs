//! The ranges a chart can be read over, on a day (#252, #497).
//!
//! **Worked out once, here, for every chart**: the mesocycle and the
//! macrocycle in force are the plans', and which days they start on is
//! `domain`'s ([`Ranges::on`]). Nothing is written.

use domain::{analytical::range::Ranges, plan::Plan};
use jiff::civil::Date;

use crate::{error::StoreError, ports::PlanStore};

/// The use case.
pub struct CurrentRanges<P> {
    plans: P,
}

crate::graph::through!(CurrentRanges<P>);

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
        Ok(of(today, &self.plans.plans().await?))
    }
}

/// The mesocycle and macrocycle `today` falls in among `plans`, each to
/// `today`: for a page that reads the plans for more than its ranges (#817).
#[must_use]
pub fn of(today: Date, plans: &[Plan]) -> Ranges {
    let mesocycles: Vec<_> = plans.iter().flat_map(Plan::mesocycle_spans).collect();
    let spans: Vec<_> = plans.iter().map(Plan::span).collect();
    Ranges::on(today, &mesocycles, &spans)
}
