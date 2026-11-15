//! Body weight and body composition, read together on one page (#786).

use application::{PlanStore as _, StoreError, next::Scope};
use domain::{
    analytical::{
        body_weight::{Aim, Series},
        composition::ComponentSeries,
        range::Ranges,
    },
    normalised::OperatorZone,
};

use crate::clock::Today;
use crate::{
    body_weight::{self, Aimed},
    composition::{self, ByMethod},
    graph::{page, read},
    next::NextFailure,
    rescheduling::{self, Plans},
    store::Store,
};

page! {
    /// What the page reads: the canonical weigh-ins, what the plans aimed
    /// them at, every normalised weigh-in's composition, and the plans the
    /// aims and the ranges are both read from (#817).
    pub struct Page {
        series: body_weight::Series,
        aims: Aimed,
        composition: ByMethod,
        plans: Plans,
        today: Today,
    }
}

/// The parts a run collecting what the page reads takes.
#[must_use]
pub fn collects() -> Scope {
    crate::graph::scope(&reads())
}

/// What the page reads, and so what its kept drawing is drawn from (#805).
#[must_use]
pub fn reads() -> application::graph::Reads {
    <Page as application::graph::Input>::reads()
}

impl Page {
    /// The page over `store`.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn open(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        Ok(Self {
            series: body_weight::series(store).into(),
            aims: body_weight::aims_over(store).into(),
            composition: composition::over(store).into(),
            plans: rescheduling::plans(store, zone).await?.into(),
            today: Today::new(zone.clone()).into(),
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

    /// What the plans intended over this macrocycle's mesocycles, and the
    /// ranges, today: both from one read of the plans (#817).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn aims_and_ranges(&self) -> Result<(Vec<Aim>, Ranges), StoreError> {
        let today = self.today.date();
        let plans = read!(self.plans.plans()).await?;
        let aims = read!(self.aims.on(today, &plans)).await?;
        Ok((aims, application::ranges::of(today, &plans)))
    }

    /// Every component anything measured, one series per method.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn by_method(&self) -> Result<Vec<ComponentSeries>, StoreError> {
        read!(self.composition.by_method()).await
    }
}
