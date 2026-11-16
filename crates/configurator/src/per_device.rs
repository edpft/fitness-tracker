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

use crate::clock::Today;
use crate::store::Spanned;
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
        nights: PerDevice<Spanned<SqliteOvernightHrvHistory>, (), Spanned<GarminRecordedDevices>, Spanned<GarminWear>>,
        ranges: Current,
        today: Today,
    }
}

page! {
    /// What the resting heart rate page reads: the days, each with the sleep
    /// record naming its watch, the register for the days before Garmin named
    /// one (#559, #569), and the ranges.
    pub struct RestingHeartRate {
        days: PerDevice<(), Spanned<SqliteRestingHeartRateHistory>, Spanned<GarminRecordedDevices>, Spanned<GarminWear>>,
        ranges: Current,
        today: Today,
    }
}

/// The parts a run collecting what the overnight HRV page reads takes.
#[must_use]
pub fn overnight_hrv_collects() -> Scope {
    crate::graph::scope(&overnight_hrv_reads())
}

/// What the overnight HRV page reads, and so what its kept drawing is drawn from (#805).
#[must_use]
pub fn overnight_hrv_reads() -> application::graph::Reads {
    <OvernightHrv as application::graph::Input>::reads()
}

/// The parts a run collecting what the resting heart rate page reads takes.
#[must_use]
pub fn resting_heart_rate_collects() -> Scope {
    crate::graph::scope(&resting_heart_rate_reads())
}

/// What the resting heart rate page reads, and so what its kept drawing is drawn from (#805).
#[must_use]
pub fn resting_heart_rate_reads() -> application::graph::Reads {
    <RestingHeartRate as application::graph::Input>::reads()
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
                store.adapter(SqliteOvernightHrvHistory::new),
                (),
                devices::register(store),
            )
            .into(),
            ranges: ranges::over(store, zone).await?.into(),
            today: Today::new(zone.clone()).into(),
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
    pub async fn ranges(&self) -> Result<Ranges, StoreError> {
        let today = self.today.date();
        read!(self.ranges, |plans| ranges::on(plans, today)).await
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
                store.adapter(SqliteRestingHeartRateHistory::new),
                devices::register(store),
            )
            .into(),
            ranges: ranges::over(store, zone).await?.into(),
            today: Today::new(zone.clone()).into(),
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
    pub async fn ranges(&self) -> Result<Ranges, StoreError> {
        let today = self.today.date();
        read!(self.ranges, |plans| ranges::on(plans, today)).await
    }
}
