//! Body composition, one series per method, over this build's store (#256),
//! and against the nutrition phase in force (#489).

use application::{
    HolidayCalendar, StoreError,
    composition::{AgainstPhase, Composition, InForce},
    next::Scope,
};
use domain::{analytical::composition::ComponentSeries, normalised::OperatorZone};
use infrastructure::{
    MyFitnessPalExportLandingStore, SpreadsheetFileLandingStore, SqliteCompositionHistory,
    WithingsMeasurementLandingStore,
};

use crate::{
    holidays::Calendars,
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

/// What the body composition chart reads: every normalised weigh-in, whose
/// components it plots by method (#559).
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    next::scope([
        Reads::Normalised(WithingsMeasurementLandingStore::STREAM),
        Reads::Normalised(SpreadsheetFileLandingStore::STREAM),
        Reads::Normalised(MyFitnessPalExportLandingStore::STREAM),
    ])
}

/// Every component anything measured, one series per method.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn by_method(store: &Store) -> Result<Vec<ComponentSeries>, StoreError> {
    Composition::new(SqliteCompositionHistory::new(store.pool().clone()))
        .by_method()
        .await
}

/// The current macrocycle's periods, each against its nutrition phase, today.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn against_phase(store: &Store, zone: &OperatorZone) -> Result<AgainstPhase, StoreError> {
    InForce::new(
        SqliteCompositionHistory::new(store.pool().clone()),
        store.plans(zone),
    )
    .on(crate::equipment::today(zone), Calendars.holidays().await)
    .await
}
