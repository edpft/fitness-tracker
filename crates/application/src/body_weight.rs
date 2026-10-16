//! Body weight as one time series (#250), over the current cycles (#252).
//!
//! **The ordering, the trend and the ranges are `domain`'s**
//! ([`Series`], [`Ranges`]); this reads the canonical weigh-ins and the
//! plans, and hands them over. Nothing is
//! written: the series re-derives from them exactly (§ 5).

use domain::analytical::body_weight::{Ranges, Series};
use jiff::civil::Date;

use crate::{
    error::StoreError,
    ports::{PlanStore, WeighInHistory},
};

/// The body-weight use case.
pub struct BodyWeight<W, P> {
    weigh_ins: W,
    plans: P,
}

/// The series, and the cycles it can be read over.
#[derive(Debug)]
pub struct View {
    pub series: Series,
    pub ranges: Ranges,
}

impl<W, P> BodyWeight<W, P>
where
    W: WeighInHistory + Sync,
    P: PlanStore + Sync,
{
    pub const fn new(weigh_ins: W, plans: P) -> Self {
        Self { weigh_ins, plans }
    }

    /// Every canonical weigh-in, oldest first, the trend through them and
    /// which way it went, and the cycles `today` falls in.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn view(&self, today: Date) -> Result<View, StoreError> {
        let mut mesocycles = Vec::new();
        let mut plans = Vec::new();
        for window in self.plans.windows().await? {
            if let Some(plan) = self.plans.named(window.name()).await? {
                mesocycles.extend(plan.mesocycle_spans());
                plans.push(plan.span());
            }
        }
        let series = Series::of(self.weigh_ins.weigh_ins().await?);
        Ok(View {
            series,
            ranges: Ranges::on(today, &mesocycles, &plans),
        })
    }
}
