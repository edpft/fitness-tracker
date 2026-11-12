//! The sessions he has done, and one of them opened, over this build's store
//! (#772).

use application::{
    CanonicalLayer, StoreError,
    history::{Day, History, Number, Session},
    next::Scope,
};
use domain::normalised::OperatorZone;
use infrastructure::SqliteCanonicalCyclingSessionStore;
use jiff::civil::Date;

use crate::{
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

/// What History reads: the canonical gym sessions and rides.
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    next::scope([
        Reads::Canonical(CanonicalLayer::Gym),
        Reads::Canonical(CanonicalLayer::Cycling),
    ])
}

/// Every day through today that holds a session, newest first.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn days(store: &Store, zone: &OperatorZone) -> Result<Vec<Day>, StoreError> {
    let today = jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date();
    history(store).days(today).await
}

/// The session `number` of those done `on`, where there is one.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn session(
    store: &Store,
    on: Date,
    number: Number,
) -> Result<Option<Session>, StoreError> {
    history(store).session(on, number).await
}

fn history(
    store: &Store,
) -> History<
    impl application::PerformedGymSessions + Send + Sync + use<>,
    SqliteCanonicalCyclingSessionStore,
> {
    History::new(
        store.canonical_gym(),
        SqliteCanonicalCyclingSessionStore::new(store.pool().clone()),
    )
}
