//! The device register, over this build's store and recordings (#427).
//!
//! **Garmin's recordings and Withings' measurements are the baseline.**
//! Peloton's account adds the monitors the Bike+ is paired with, by their ANT+
//! numbers. Another source of what a device recorded is another reader here,
//! and nothing above this changes. Peloton names no unit of the bike itself,
//! so the Bike+ is a device the operator adds.

use application::{
    StoreError,
    devices::{DeviceRegister, StatementRefused},
};
use domain::{
    device::{AddedDevice, Device, DeviceStatement},
    normalised::OperatorZone,
};
use infrastructure::{
    GarminRecordedDevices, PelotonRecordedDevices, SqliteDeviceStatementStore,
    WithingsRecordedDevices,
};
use jiff::Timestamp;

use crate::store::Store;

type Recorded = (
    (GarminRecordedDevices, WithingsRecordedDevices),
    PelotonRecordedDevices,
);

fn register(
    store: &Store,
    zone: &OperatorZone,
) -> Result<DeviceRegister<Recorded, SqliteDeviceStatementStore>, StoreError> {
    let withings =
        WithingsRecordedDevices::new(store.pool().clone(), zone.clone()).map_err(|error| {
            StoreError::Corrupt {
                detail: error.to_string(),
            }
        })?;
    Ok(DeviceRegister::new(
        (
            (
                GarminRecordedDevices::new(store.pool().clone(), zone.clone()),
                withings,
            ),
            PelotonRecordedDevices::new(store.pool().clone(), zone.clone()),
        ),
        SqliteDeviceStatementStore::new(store.pool().clone()),
    ))
}

/// Every device, ordered by first use.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn devices(store: &Store, zone: &OperatorZone) -> Result<Vec<Device>, StoreError> {
    register(store, zone)?.devices().await
}

/// Keep a statement about a device, made now.
///
/// # Errors
///
/// [`StatementRefused`] if the register would not apply it or the store is
/// unavailable.
pub async fn state(
    store: &Store,
    zone: &OperatorZone,
    statement: DeviceStatement,
) -> Result<Vec<Device>, StatementRefused> {
    register(store, zone)?
        .state(Timestamp::now(), statement)
        .await
}

/// Keep a device no recording names, added now.
///
/// # Errors
///
/// [`StoreError`] if the store is unavailable.
pub async fn add(
    store: &Store,
    zone: &OperatorZone,
    added: AddedDevice,
) -> Result<Vec<Device>, StoreError> {
    register(store, zone)?.add(Timestamp::now(), added).await
}
