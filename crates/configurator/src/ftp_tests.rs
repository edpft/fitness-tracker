//! The FTP tests, in watts and over body weight, over this build's store
//! (#469).

use application::{
    CanonicalLayer, StoreError,
    devices::DeviceRegister,
    ftp_tests::{FtpTests, TestHeartRates},
    next::Scope,
};
use domain::analytical::ftp_test::{Test, TestHeartRate};
use infrastructure::{
    GarminRecordedDevices, GarminWear, SqliteCanonicalCyclingSessionStore, SqliteFtpHistory,
};

use crate::{
    devices,
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

/// What the FTP tests page reads: the canonical rides, the FTP derived with
/// them, the canonical weigh-ins, and which watch was in use (#548).
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    next::scope(
        [
            Reads::Canonical(CanonicalLayer::Cycling),
            Reads::Canonical(CanonicalLayer::WeighIns),
        ]
        .into_iter()
        .chain(devices::READS),
    )
}

/// Every test, oldest first.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn tests(store: &Store) -> Result<Vec<Test>, StoreError> {
    let pool = store.pool();
    FtpTests::new(
        SqliteCanonicalCyclingSessionStore::new(pool.clone()),
        SqliteFtpHistory::new(pool.clone()),
        store.weigh_ins(),
    )
    .tests()
    .await
}

/// The heart rate each test was ridden at, oldest first (#548).
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn heart_rates(store: &Store) -> Result<Vec<TestHeartRate>, StoreError> {
    let pool = store.pool();
    TestHeartRates::new(
        SqliteCanonicalCyclingSessionStore::new(pool.clone()),
        DeviceRegister::new(
            GarminRecordedDevices::new(pool.clone()),
            GarminWear::new(pool.clone()),
        ),
    )
    .heart_rates()
    .await
}
