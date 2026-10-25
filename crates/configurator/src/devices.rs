//! Which device sent a reading, over this build's store (#427).
//!
//! **Garmin's normalised activities are the record**, and its daily heart rate
//! says which days a watch was worn (#569). Garmin names every device that
//! wrote a recording and every sensor paired to it. Peloton's monitor is the
//! ride's own, composed into it from Peloton's account, so it is not read here.
//! Another source of what a device recorded is another reader here, and nothing
//! above this changes.

use application::{StoreError, devices::DeviceRegister};
use domain::device::Register;
use infrastructure::{
    GarminActivityLandingStore, GarminDailyHeartRateLandingStore, GarminRecordedDevices, GarminWear,
};

use crate::{next::Reads, store::Store};

/// What the register reads: Garmin's activities and days, normalised (#569).
pub const READS: [Reads; 2] = [
    Reads::Normalised(GarminActivityLandingStore::STREAM),
    Reads::Normalised(GarminDailyHeartRateLandingStore::STREAM),
];

/// Every device the record identifies, and the days a watch was worn.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn register(store: &Store) -> Result<Register, StoreError> {
    DeviceRegister::new(
        GarminRecordedDevices::new(store.pool().clone()),
        GarminWear::new(store.pool().clone()),
    )
    .register()
    .await
}
