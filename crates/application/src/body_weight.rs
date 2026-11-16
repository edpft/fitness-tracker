//! Body weight as one time series (#250).
//!
//! **The ordering and the trend are `domain`'s** ([`Series`]); this reads the
//! canonical weigh-ins and hands them over. Nothing is written: the series
//! re-derives from them exactly (§ 5). The ranges it is read over are every
//! chart's ([`crate::ranges`]).

use domain::{
    analytical::{
        Weighed,
        body_weight::{Aim, Series, aims},
    },
    macrocycle::Macrocycle,
    plan::Plan,
    schedule::Holidays,
};
use jiff::civil::Date;

use crate::{error::StoreError, graph::through, ports::WeighInHistory};

/// The body-weight use case.
pub struct BodyWeight<W> {
    weigh_ins: W,
}

through!(BodyWeight<W>);

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
        Ok(Series::of(self.weigh_ins().await?))
    }

    /// Every canonical weigh-in, as the store holds them: for a page whose
    /// other readings are over body weight too, which reads them once (#817).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn weigh_ins(&self) -> Result<Vec<Weighed>, StoreError> {
        self.weigh_ins.weigh_ins().await
    }
}

/// What `plans`, as they now stand, intended over the mesocycles of the
/// macrocycle `today` falls in (#689).
///
/// None where the school calendar could not be read, `holidays` being
/// `None`, or `today` is past its reach, since then there is no macrocycle
/// to assume a phase from.
#[must_use]
pub fn aims_on(today: Date, plans: &[Plan], holidays: Option<&Holidays>) -> Vec<Aim> {
    let Some(macrocycle) = holidays.and_then(|holidays| Macrocycle::on(today, holidays)) else {
        return Vec::new();
    };
    let mesocycles: Vec<_> = plans.iter().flat_map(Plan::mesocycle_spans).collect();
    aims(macrocycle, &mesocycles, today)
}
