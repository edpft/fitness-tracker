//! A night's sleep, how long and when, one series per watch, over this
//! build's store (#255).

use application::{StoreError, next::Scope, sleep::Sleeping};
use domain::{
    analytical::{range::Ranges, sleep::Trended},
    normalised::OperatorZone,
};
use infrastructure::{GarminRecordedDevices, GarminWear, SqliteSleepHistory};

use crate::store::Spanned;
use crate::{
    devices,
    graph::{page, read},
    next::NextFailure,
    ranges::{self, Current},
    store::Store,
};

page! {
    /// What the sleep page reads: the nights, each naming its watch, the
    /// register for the nights before Garmin named one (#559, #569), and the
    /// ranges.
    pub struct Page {
        nights: Sleeping<Spanned<SqliteSleepHistory>, Spanned<GarminRecordedDevices>, Spanned<GarminWear>>,
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
        Ok(Self {
            nights: Sleeping::new(
                store.adapter(SqliteSleepHistory::new),
                devices::register(store),
            )
            .into(),
            ranges: ranges::over(store, zone).await?.into(),
        })
    }

    /// Every night's sleep, one series per watch, with its trends.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn nights(&self) -> Result<Vec<Trended>, StoreError> {
        read!(self.nights.nights()).await
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
