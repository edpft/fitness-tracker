//! Heart rate per power zone, over this build's store (#152).

use application::{StoreError, devices::DeviceRegister, heart_rate::HeartRatePerZone};
use domain::{analytical::heart_rate::RideHeartRate, normalised::OperatorZone};
use infrastructure::{
    GarminRecordedDevices, GarminWear, PelotonRecordedDevices, SqliteCanonicalCyclingSessionStore,
    SqliteFtpHistory,
};

use crate::store::Store;

/// Every main ride's heart rate per power zone, oldest first.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn rides(store: &Store, zone: &OperatorZone) -> Result<Vec<RideHeartRate>, StoreError> {
    let pool = store.pool();
    HeartRatePerZone::new(
        SqliteCanonicalCyclingSessionStore::new(pool.clone()),
        SqliteFtpHistory::new(pool.clone()),
        DeviceRegister::new(
            (
                GarminRecordedDevices::new(pool.clone(), zone.clone()),
                PelotonRecordedDevices::new(pool.clone(), zone.clone()),
            ),
            GarminWear::new(pool.clone()),
        ),
    )
    .rides()
    .await
}
