//! Overnight HRV and resting heart rate, one series per device, over this
//! build's store (#254).

use application::{StoreError, devices::DeviceRegister, next::Scope, per_device::PerDevice};
use domain::analytical::{hrv::Trended, resting};
use infrastructure::{
    GarminDailyHeartRateLandingStore, GarminHrvLandingStore, GarminRecordedDevices, GarminWear,
    SqliteOvernightHrvHistory, SqliteRestingHeartRateHistory,
};

use crate::{
    devices,
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

fn per_device(
    store: &Store,
) -> PerDevice<
    SqliteOvernightHrvHistory,
    SqliteRestingHeartRateHistory,
    GarminRecordedDevices,
    GarminWear,
> {
    let pool = store.pool();
    PerDevice::new(
        SqliteOvernightHrvHistory::new(pool.clone()),
        SqliteRestingHeartRateHistory::new(pool.clone()),
        DeviceRegister::new(
            GarminRecordedDevices::new(pool.clone()),
            GarminWear::new(pool.clone()),
        ),
    )
}

/// What the overnight HRV chart reads: the nights, each with the sleep record
/// naming its watch, and Garmin's recordings, which say what it is called
/// (#559, #569).
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn overnight_hrv_collects() -> Result<Scope, WiringError> {
    next::scope(
        std::iter::once(Reads::Normalised(GarminHrvLandingStore::STREAM)).chain(devices::READS),
    )
}

/// What the resting heart rate chart reads: the days, each with the sleep
/// record naming its watch, and the register for the days before Garmin named
/// one (#559, #569).
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn resting_heart_rate_collects() -> Result<Scope, WiringError> {
    next::scope(
        std::iter::once(Reads::Normalised(GarminDailyHeartRateLandingStore::STREAM))
            .chain(devices::READS),
    )
}

/// Last night's average HRV, one series per watch, and the trend through each.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn overnight_hrv_trend(store: &Store) -> Result<Vec<Trended>, StoreError> {
    per_device(store).overnight_hrv_trend().await
}

/// Each day's resting heart rate, one series per watch, with its slow line
/// and the nights it flags.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn resting_heart_rate_trend(store: &Store) -> Result<Vec<resting::Trended>, StoreError> {
    per_device(store).resting_heart_rate_trend().await
}
