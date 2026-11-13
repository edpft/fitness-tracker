//! Pulse wave velocity, one series per algorithm, over this build's store
//! (#471).

use application::{StoreError, next::Scope, pulse_wave_velocity::PulseWaveVelocities};
use domain::{
    analytical::{pulse_wave_velocity::Series, range::Ranges},
    normalised::OperatorZone,
};
use infrastructure::SqlitePulseWaveVelocityHistory;

use crate::{
    graph::page,
    next::NextFailure,
    ranges::{self, Current},
    store::Store,
};

page! {
    /// What the pulse wave velocity page reads: the normalised Body Scan
    /// weigh-ins, whose vascular part holds it, and the ranges.
    pub struct Page {
        series: PulseWaveVelocities<SqlitePulseWaveVelocityHistory>,
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
            series: PulseWaveVelocities::new(SqlitePulseWaveVelocityHistory::new(
                store.pool().clone(),
            )),
            ranges: ranges::over(store, zone).await?,
        })
    }

    /// Every reading, one series per algorithm, with the line through it.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn by_algorithm(&self) -> Result<Vec<Series>, StoreError> {
        self.series.by_algorithm().await
    }

    /// The ranges today.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn ranges(&self, zone: &OperatorZone) -> Result<Ranges, StoreError> {
        ranges::today(&self.ranges, zone).await
    }
}
