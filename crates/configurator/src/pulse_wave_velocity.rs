//! Pulse wave velocity, one series per algorithm, over this build's store
//! (#471).

use application::{StoreError, next::Scope, pulse_wave_velocity::PulseWaveVelocities};
use domain::analytical::pulse_wave_velocity::Series;
use infrastructure::{SqlitePulseWaveVelocityHistory, WithingsMeasurementLandingStore};

use crate::{
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

/// What the pulse wave velocity chart reads: the normalised Body Scan
/// weigh-ins, whose vascular part holds it.
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    next::scope([Reads::Normalised(WithingsMeasurementLandingStore::STREAM)])
}

/// Every reading, one series per algorithm, with the line through it.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn by_algorithm(store: &Store) -> Result<Vec<Series>, StoreError> {
    PulseWaveVelocities::new(SqlitePulseWaveVelocityHistory::new(store.pool().clone()))
        .by_algorithm()
        .await
}
