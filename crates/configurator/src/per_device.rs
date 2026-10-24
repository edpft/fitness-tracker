//! Overnight HRV and resting heart rate, one series per device, over this
//! build's store (#254).

use application::{StoreError, devices::DeviceRegister, next::Scope, per_device::PerDevice};
use domain::{
    analytical::{hrv::Trended, resting},
    normalised::OperatorZone,
};
use infrastructure::{
    GarminDailyHeartRateLandingStore, GarminHrvLandingStore, GarminRecordedDevices, GarminWear,
    PelotonRecordedDevices, SqliteOvernightHrvHistory, SqliteRestingHeartRateHistory,
};

use crate::{
    devices,
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

fn per_device(
    store: &Store,
    zone: &OperatorZone,
) -> PerDevice<
    SqliteOvernightHrvHistory,
    SqliteRestingHeartRateHistory,
    (GarminRecordedDevices, PelotonRecordedDevices),
    GarminWear,
> {
    let pool = store.pool();
    PerDevice::new(
        SqliteOvernightHrvHistory::new(pool.clone()),
        SqliteRestingHeartRateHistory::new(pool.clone()),
        DeviceRegister::new(
            (
                GarminRecordedDevices::new(pool.clone(), zone.clone()),
                PelotonRecordedDevices::new(pool.clone(), zone.clone()),
            ),
            GarminWear::new(pool.clone()),
        ),
    )
}

/// What the overnight HRV chart reads: the nights, and which watch each was
/// recorded by (#559).
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn overnight_hrv_collects() -> Result<Scope, WiringError> {
    next::scope(
        std::iter::once(Reads::Normalised(GarminHrvLandingStore::STREAM)).chain(devices::READS),
    )
}

/// What the resting heart rate chart reads: the days, and which watch each
/// was recorded by (#559).
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
pub async fn overnight_hrv_trend(
    store: &Store,
    zone: &OperatorZone,
) -> Result<Vec<Trended>, StoreError> {
    per_device(store, zone).overnight_hrv_trend().await
}

/// Each day's resting heart rate, one series per watch, with its slow line
/// and the nights it flags.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn resting_heart_rate_trend(
    store: &Store,
    zone: &OperatorZone,
) -> Result<Vec<resting::Trended>, StoreError> {
    per_device(store, zone).resting_heart_rate_trend().await
}
