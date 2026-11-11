//! Is what I'm doing working?, for the macrocycle in force, over this build's
//! store (#517).

use application::{
    CanonicalLayer, StoreError,
    devices::DeviceRegister,
    next::Scope,
    working::{IsItWorking, View},
};
use domain::normalised::OperatorZone;
use infrastructure::{
    GarminActivityLandingStore, GarminDailyHeartRateLandingStore, GarminDailySummaryLandingStore,
    GarminHrvLandingStore, GarminRecordedDevices, GarminWear, SqliteOvernightHrvHistory,
    SqliteRestingHeartRateHistory, SqliteStressHistory, SqliteTrainingDone,
};

use crate::{
    devices, ftp_tests,
    next::{self, Reads},
    one_rep_max,
    store::Store,
    wiring::WiringError,
};

/// What the page reads.
///
/// The three response readings, each normalised; the training done, which is
/// the canonical gym and cycling sessions and the padel games Garmin's
/// activities derive; the canonical weigh-ins an FTP test is read beside; and
/// the register.
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    next::scope(
        [
            Reads::Normalised(GarminHrvLandingStore::STREAM),
            Reads::Normalised(GarminDailyHeartRateLandingStore::STREAM),
            Reads::Normalised(GarminDailySummaryLandingStore::STREAM),
            Reads::Normalised(GarminActivityLandingStore::STREAM),
            Reads::Canonical(CanonicalLayer::Gym),
            Reads::Canonical(CanonicalLayer::Cycling),
            Reads::Canonical(CanonicalLayer::WeighIns),
        ]
        .into_iter()
        .chain(devices::READS),
    )
}

/// The macrocycle in force, through today.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn view(store: &Store, zone: &OperatorZone) -> Result<View, StoreError> {
    let today = jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date();
    let pool = store.pool();
    let ftp = ftp_tests::tests(store).await?;
    let strength = one_rep_max::tests(store, zone).await?;
    IsItWorking::new(
        SqliteOvernightHrvHistory::new(pool.clone()),
        SqliteRestingHeartRateHistory::new(pool.clone()),
        SqliteStressHistory::new(pool.clone()),
        SqliteTrainingDone::new(pool.clone()),
        DeviceRegister::new(
            GarminRecordedDevices::new(pool.clone()),
            GarminWear::new(pool.clone()),
        ),
        store.plans(zone),
    )
    .view(today, &ftp, &strength)
    .await
}
