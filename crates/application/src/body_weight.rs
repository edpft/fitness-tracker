//! Body weight as one time series (#250).
//!
//! **The ordering and the trend are `domain`'s** ([`Series`]); this reads the
//! canonical weigh-ins and hands them over. Nothing is written: the series
//! re-derives from them exactly (§ 5). The ranges it is read over are every
//! chart's ([`crate::ranges`]).

use domain::{
    analytical::body_weight::{Aim, Series, aims},
    macrocycle::Macrocycle,
};
use jiff::civil::Date;

use crate::{
    error::StoreError,
    graph::through,
    ports::{HolidayCalendar, PlanStore, WeighInHistory},
};

/// The body-weight use case.
pub struct BodyWeight<W> {
    weigh_ins: W,
}

impl<W> BodyWeight<W>
where
    W: WeighInHistory + Sync,
{
    pub const fn new(weigh_ins: W) -> Self {
        Self { weigh_ins }
    }

    /// Every canonical weigh-in, oldest first, and the trend through them.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn series(&self) -> Result<Series, StoreError> {
        Ok(Series::of(self.weigh_ins.weigh_ins().await?))
    }
}

/// What the plans intended over the current macrocycle's mesocycles (#689).
pub struct Aims<P, H> {
    plans: P,
    holidays: H,
}

through!(BodyWeight<W>, Aims<P, H>);

impl<P, H> Aims<P, H>
where
    P: PlanStore + Sync,
    H: HolidayCalendar + Sync,
{
    pub const fn new(plans: P, holidays: H) -> Self {
        Self { plans, holidays }
    }

    /// Each started mesocycle's aim on `today`; none where the school
    /// calendar could not be read or `today` is past its reach, since then
    /// there is no macrocycle to assume a phase from.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn on(&self, today: Date) -> Result<Vec<Aim>, StoreError> {
        let Some(macrocycle) = self
            .holidays
            .holidays()
            .await
            .ok()
            .and_then(|holidays| Macrocycle::on(today, &holidays))
        else {
            return Ok(Vec::new());
        };
        let mut mesocycles = Vec::new();
        for window in self.plans.windows().await? {
            if let Some(plan) = self.plans.named(window.name()).await? {
                mesocycles.extend(plan.mesocycle_spans());
            }
        }
        Ok(aims(macrocycle, &mesocycles, today))
    }
}
