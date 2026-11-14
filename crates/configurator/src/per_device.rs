//! Overnight HRV and resting heart rate, one series per device, over this
//! build's store (#254).

use application::{StoreError, next::Scope, per_device::PerDevice};
use domain::{
    analytical::{hrv::Trended, range::Ranges, resting},
    normalised::OperatorZone,
};
use infrastructure::{
    GarminRecordedDevices, GarminWear, SqliteOvernightHrvHistory, SqliteRestingHeartRateHistory,
};

use crate::{
    devices,
    graph::{page, read},
    next::NextFailure,
    ranges::{self, Current},
    store::Store,
};

page! {
    /// What the overnight HRV page reads: the nights, each with the sleep
    /// record naming its watch, Garmin's recordings, which say what it is
    /// called (#559, #569), and the ranges.
    pub struct OvernightHrv {
        nights: PerDevice<SqliteOvernightHrvHistory, (), GarminRecordedDevices, GarminWear>,
        ranges: Current,
    }
}

page! {
    /// What the resting heart rate page reads: the days, each with the sleep
    /// record naming its watch, the register for the days before Garmin named
    /// one (#559, #569), and the ranges.
    pub struct RestingHeartRate {
        days: PerDevice<(), SqliteRestingHeartRateHistory, GarminRecordedDevices, GarminWear>,
        ranges: Current,
    }
}

/// The parts a run collecting what the overnight HRV page reads takes.
#[must_use]
pub fn overnight_hrv_collects() -> Scope {
    crate::graph::collects::<OvernightHrv>()
}

/// The parts a run collecting what the resting heart rate page reads takes.
#[must_use]
pub fn resting_heart_rate_collects() -> Scope {
    crate::graph::collects::<RestingHeartRate>()
}

impl OvernightHrv {
    /// The page over `store`.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn open(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        Ok(Self {
            nights: PerDevice::new(
                SqliteOvernightHrvHistory::new(store.pool().clone()),
                (),
                devices::register(store),
            )
            .into(),
            ranges: ranges::over(store, zone).await?.into(),
        })
    }

    /// Last night's average HRV, one series per watch, and the trend through
    /// each.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn trend(&self) -> Result<Vec<Trended>, StoreError> {
        read!(self.nights.overnight_hrv_trend()).await
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

impl RestingHeartRate {
    /// The page over `store`.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn open(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        Ok(Self {
            days: PerDevice::new(
                (),
                SqliteRestingHeartRateHistory::new(store.pool().clone()),
                devices::register(store),
            )
            .into(),
            ranges: ranges::over(store, zone).await?.into(),
        })
    }

    /// Each day's resting heart rate, one series per watch, with its slow line
    /// and the nights it flags.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn trend(&self) -> Result<Vec<resting::Trended>, StoreError> {
        read!(self.days.resting_heart_rate_trend()).await
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
