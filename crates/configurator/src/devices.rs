//! Which device sent a reading, over this build's store (#427).
//!
//! **Garmin's normalised activities are the record**, and its daily heart rate
//! says which days a watch was worn (#569). Garmin names every device that
//! wrote a recording and every sensor paired to it. Peloton's monitor is the
//! ride's own, composed into it from Peloton's account, so it is not read here.
//! Another source of what a device recorded is another reader here, and nothing
//! above this changes.

use application::devices::DeviceRegister;
use infrastructure::{GarminRecordedDevices, GarminWear};

use crate::store::Spanned;
use crate::store::Store;

/// Every device the record identifies, and the days a watch was worn.
pub type Register = DeviceRegister<Spanned<GarminRecordedDevices>, Spanned<GarminWear>>;

/// The register over `store`.
pub(crate) fn register(store: &Store) -> Register {
    DeviceRegister::new(
        store.adapter(GarminRecordedDevices::new),
        store.adapter(GarminWear::new),
    )
}
