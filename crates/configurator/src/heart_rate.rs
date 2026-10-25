//! Heart rate per power zone, over this build's store (#152).

use application::{
    CanonicalLayer, StoreError, devices::DeviceRegister, heart_rate::HeartRatePerZone, next::Scope,
};
use domain::analytical::heart_rate::RideHeartRate;
use infrastructure::{
    GarminRecordedDevices, GarminWear, SqliteCanonicalCyclingSessionStore, SqliteFtpHistory,
};

use crate::{
    devices,
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

/// What the heart rate chart reads: the canonical rides, the FTP derived with
/// them and the monitor each was ridden with, and which watch was in use (#559,
/// #569).
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    next::scope(std::iter::once(Reads::Canonical(CanonicalLayer::Cycling)).chain(devices::READS))
}

/// Every main ride's heart rate per power zone, oldest first.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn rides(store: &Store) -> Result<Vec<RideHeartRate>, StoreError> {
    let pool = store.pool();
    HeartRatePerZone::new(
        SqliteCanonicalCyclingSessionStore::new(pool.clone()),
        SqliteFtpHistory::new(pool.clone()),
        DeviceRegister::new(
            GarminRecordedDevices::new(pool.clone()),
            GarminWear::new(pool.clone()),
        ),
    )
    .rides()
    .await
}
