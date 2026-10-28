//! A night's sleep, how long and when, one series per watch, over this
//! build's store (#255).

use application::{StoreError, devices::DeviceRegister, next::Scope, sleep::Sleeping};
use domain::analytical::sleep::Trended;
use infrastructure::{
    GarminRecordedDevices, GarminSleepLandingStore, GarminWear, SqliteSleepHistory,
};

use crate::{
    devices,
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

/// What the sleep charts read: the nights, each naming its watch, and the
/// register for the nights before Garmin named one (#559, #569).
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    next::scope(
        std::iter::once(Reads::Normalised(GarminSleepLandingStore::STREAM)).chain(devices::READS),
    )
}

/// Every night's sleep, one series per watch, with its trends.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn nights(store: &Store) -> Result<Vec<Trended>, StoreError> {
    let pool = store.pool();
    Sleeping::new(
        SqliteSleepHistory::new(pool.clone()),
        DeviceRegister::new(
            GarminRecordedDevices::new(pool.clone()),
            GarminWear::new(pool.clone()),
        ),
    )
    .nights()
    .await
}
