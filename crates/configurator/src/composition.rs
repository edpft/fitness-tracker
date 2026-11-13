//! Body composition, one series per method, over this build's store (#256).

use application::{StoreError, composition::Composition, next::Scope};
use domain::analytical::composition::ComponentSeries;
use infrastructure::{
    MyFitnessPalExportLandingStore, SpreadsheetFileLandingStore, SqliteCompositionHistory,
    WithingsMeasurementLandingStore,
};

use crate::{
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
