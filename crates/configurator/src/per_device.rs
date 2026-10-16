//! Overnight HRV and resting heart rate, one series per device, over this
//! build's store (#254).

use application::{StoreError, devices::DeviceRegister, per_device::PerDevice};
use domain::{
    analytical::{hrv::Trended, resting},
    normalised::OperatorZone,
};
use infrastructure::{
    GarminRecordedDevices, GarminWear, PelotonRecordedDevices, SqliteOvernightHrvHistory,
    SqliteRestingHeartRateHistory,
};

use crate::store::Store;

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
