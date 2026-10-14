//! Which device sent a reading, over this build's store (#427).
//!
//! **Garmin's recordings and Peloton's account are the record**, and Garmin's
//! daily heart rate says which days a watch was worn. Garmin names every
//! device that wrote a recording and every sensor paired to it; Peloton names
//! the monitor the Bike+ is paired with. Another source of what a device
//! recorded is another reader here, and nothing above this changes.

use application::{StoreError, devices::DeviceRegister};
use domain::{device::Register, normalised::OperatorZone};
use infrastructure::{GarminRecordedDevices, GarminWear, PelotonRecordedDevices};

use crate::store::Store;

/// Every device the record identifies, and the days a watch was worn.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn register(store: &Store, zone: &OperatorZone) -> Result<Register, StoreError> {
    DeviceRegister::new(
        (
            GarminRecordedDevices::new(store.pool().clone(), zone.clone()),
            PelotonRecordedDevices::new(store.pool().clone(), zone.clone()),
        ),
        GarminWear::new(store.pool().clone()),
    )
    .register()
    .await
}
