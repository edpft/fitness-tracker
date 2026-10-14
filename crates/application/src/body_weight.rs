//! Body weight as one time series (#250), over the current cycles (#252).
//!
//! **The ordering, the trend and the ranges are `domain`'s**
//! ([`Series`], [`Ranges`]); this reads the canonical weigh-ins, the planned
//! mesocycles and the school calendar, and hands them over. Nothing is
//! written: the series re-derives from them exactly (§ 5).

use domain::{
    analytical::body_weight::{Ranges, Series},
    macrocycle::Macrocycle,
};
use jiff::civil::Date;

use crate::{
    error::{SourceError, StoreError},
    ports::{HolidayCalendar, PlanStore, WeighInHistory},
};

/// The body-weight use case.
pub struct BodyWeight<W, P, H> {
    weigh_ins: W,
    plans: P,
    calendar: H,
}

/// The series, and the cycles it can be read over.
#[derive(Debug)]
pub struct View {
    pub series: Series,
    pub ranges: Ranges,
    /// Why there is no macrocycle, when the school calendar could not be read.
    pub calendar: Option<SourceError>,
}

impl<W, P, H> BodyWeight<W, P, H>
where
    W: WeighInHistory + Sync,
    P: PlanStore + Sync,
    H: HolidayCalendar + Sync,
{
    pub const fn new(weigh_ins: W, plans: P, calendar: H) -> Self {
        Self {
            weigh_ins,
            plans,
            calendar,
        }
    }

    /// Every canonical weigh-in, oldest first, the trend through them and
    /// which way it went, and the cycles `today` falls in.
    ///
    /// **An unreadable calendar costs the macrocycle, not the page**: it is a
    /// source on the network, and the weigh-ins are not.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn view(&self, today: Date) -> Result<View, StoreError> {
        let mut mesocycles = Vec::new();
        for window in self.plans.windows().await? {
            if let Some(plan) = self.plans.named(window.name()).await? {
                mesocycles.extend(plan.mesocycle_spans());
            }
        }
        let series = Series::of(self.weigh_ins.weigh_ins().await?);
        let (macrocycle, calendar) = match self.calendar.holidays().await {
            Ok(holidays) => (Macrocycle::on(today, &holidays), None),
            Err(error) => (None, Some(error)),
        };
        Ok(View {
            series,
            ranges: Ranges::on(today, &mesocycles, macrocycle.as_ref()),
            calendar,
        })
    }
}
