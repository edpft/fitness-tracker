//! Body weight and body composition, read together on one page (#786).

use application::{StoreError, next::Scope};
use domain::{
    analytical::{
        body_weight::{Aim, Series},
        composition::ComponentSeries,
        range::Ranges,
    },
    normalised::OperatorZone,
};

use crate::{
    body_weight::{self, Aimed},
    composition::{self, ByMethod},
    graph::{page, read},
    next::NextFailure,
    ranges::{self, Current},
    rescheduling,
    store::Store,
};

page! {
    /// What the page reads: the canonical weigh-ins, what the plans aimed
    /// them at, every normalised weigh-in's composition, and the ranges.
    pub struct Page {
        series: body_weight::Series,
        aims: Aimed,
        composition: ByMethod,
        ranges: Current,
    }
}

/// The parts a run collecting what the page reads takes.
#[must_use]
pub fn collects() -> Scope {
    crate::graph::collects::<Page>()
}

impl Page {
    /// The page over `store`.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn open(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        let plans = rescheduling::plans(store, zone).await?;
        Ok(Self {
            series: body_weight::series(store).into(),
            aims: body_weight::aims_over(store, plans.clone()).into(),
            composition: composition::over(store).into(),
            ranges: application::ranges::CurrentRanges::new(plans).into(),
        })
    }

    /// Every canonical weigh-in, the trend through them.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn series(&self) -> Result<Series, StoreError> {
        read!(self.series.series()).await
    }

    /// What the plans intended over this macrocycle's mesocycles, today.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn aims(&self, zone: &OperatorZone) -> Result<Vec<Aim>, StoreError> {
        read!(self.aims.on(crate::equipment::today(zone))).await
    }

    /// Every component anything measured, one series per method.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn by_method(&self) -> Result<Vec<ComponentSeries>, StoreError> {
        read!(self.composition.by_method()).await
    }

    /// The ranges today.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn ranges(&self, zone: &OperatorZone) -> Result<Ranges, StoreError> {
        read!(self.ranges, |of| ranges::today(of, zone)).await
    }
}
