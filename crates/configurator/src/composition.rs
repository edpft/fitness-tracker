//! Body composition, one series per method, over this build's store (#256),
//! and against the nutrition phase in force (#489).

use application::{
    HolidayCalendar, StoreError,
    composition::{AgainstPhase, Composition, InForce},
};
use domain::{analytical::composition::ComponentSeries, normalised::OperatorZone};
use infrastructure::SqliteCompositionHistory;

use crate::{holidays::Calendars, store::Store};

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
