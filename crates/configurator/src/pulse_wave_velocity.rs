//! Pulse wave velocity, one series per algorithm, over this build's store
//! (#471).

use application::{StoreError, next::Scope, pulse_wave_velocity::PulseWaveVelocities};
use domain::{
    analytical::{pulse_wave_velocity::Series, range::Ranges},
    normalised::OperatorZone,
};
use infrastructure::SqlitePulseWaveVelocityHistory;

use crate::store::Spanned;
use crate::{
    graph::{page, read},
    next::NextFailure,
    ranges::{self, Current},
    store::Store,
};

page! {
    /// What the pulse wave velocity page reads: the normalised Body Scan
    /// weigh-ins, whose vascular part holds it, and the ranges.
    pub struct Page {
        series: PulseWaveVelocities<Spanned<SqlitePulseWaveVelocityHistory>>,
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
            series: PulseWaveVelocities::new(store.adapter(SqlitePulseWaveVelocityHistory::new))
                .into(),
            ranges: ranges::over(store, zone).await?.into(),
        })
    }

    /// Every reading, one series per algorithm, with the line through it.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn by_algorithm(&self) -> Result<Vec<Series>, StoreError> {
        read!(self.series.by_algorithm()).await
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
