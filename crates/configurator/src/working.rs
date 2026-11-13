//! Is what I'm doing working?, for the macrocycle in force, over this build's
//! store (#517): what Progress shows (#769).

use application::{
    CanonicalLayer,
    devices::DeviceRegister,
    next::Scope,
    working::{IsItWorking, Readings, View},
};
use domain::normalised::OperatorZone;
use infrastructure::{
    GarminActivityLandingStore, GarminDailyHeartRateLandingStore, GarminDailySummaryLandingStore,
    GarminHrvLandingStore, GarminRecordedDevices, GarminWear, MyFitnessPalExportLandingStore,
    SpreadsheetFileLandingStore, SqliteOvernightHrvHistory, SqliteRestingHeartRateHistory,
    SqliteStressHistory, SqliteTrainingDone, WithingsMeasurementLandingStore,
};

use crate::{
    body_weight, composition, devices, ftp_tests, heart_rate,
    next::{self, NextFailure, Reads},
    one_rep_max, rescheduling,
    store::Store,
    wiring::WiringError,
};

/// What the page reads.
///
/// The three response readings, each normalised; the training done, which is
/// the canonical gym and cycling sessions and the padel games Garmin's
/// activities derive; the canonical weigh-ins an FTP test and body weight are
/// read from; every normalised weigh-in, for body composition; and the
/// register.
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
            Reads::Normalised(WithingsMeasurementLandingStore::STREAM),
            Reads::Normalised(SpreadsheetFileLandingStore::STREAM),
            Reads::Normalised(MyFitnessPalExportLandingStore::STREAM),
        ]
        .into_iter()
        .chain(devices::READS),
    )
}

/// The macrocycle in force, through today, its mesocycles as the plans now
/// stand (#800).
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub async fn view(store: &Store, zone: &OperatorZone) -> Result<View, NextFailure> {
    let today = jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date();
    let pool = store.pool();
    let plans = rescheduling::plans(store, zone).await?;
    let ftp = ftp_tests::tests(store).await?;
    let strength = one_rep_max::tests(store, zone).await?;
    let estimated = one_rep_max::view_over(store, plans.clone(), zone)
        .await?
        .lifts;
    let at_power =
        domain::analytical::heart_rate_at_power::trended(&heart_rate::rides(store).await?);
    let weight = body_weight::series(store).await?;
    let aims = body_weight::aims_over(store, plans.clone(), zone).await?;
    let composition = composition::by_method(store).await?;
    Ok(IsItWorking::new(
        SqliteOvernightHrvHistory::new(pool.clone()),
        SqliteRestingHeartRateHistory::new(pool.clone()),
        SqliteStressHistory::new(pool.clone()),
        SqliteTrainingDone::new(pool.clone()),
        DeviceRegister::new(
            GarminRecordedDevices::new(pool.clone()),
            GarminWear::new(pool.clone()),
        ),
        plans,
    )
    .view(
        today,
        &Readings {
            ftp: &ftp,
            strength: &strength,
            estimated: &estimated,
            at_power: &at_power,
            weight: &weight,
            aims: &aims,
            composition: &composition,
        },
    )
    .await?)
}
