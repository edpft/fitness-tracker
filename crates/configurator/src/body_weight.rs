//! Body weight as one time series, over this build's store (#250).

use application::{
    CanonicalLayer, HolidayCalendar, StoreError,
    body_weight::{Aims, BodyWeight},
    next::Scope,
};
use domain::{
    analytical::body_weight::{Aim, Series},
    normalised::OperatorZone,
};

use crate::{
    holidays::Calendars,
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

/// What the body weight chart reads: the canonical weigh-ins (#559).
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    next::scope([Reads::Canonical(CanonicalLayer::WeighIns)])
}

/// Every canonical weigh-in, the trend through them and how fast it moved.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn series(store: &Store) -> Result<Series, StoreError> {
    BodyWeight::new(store.weigh_ins()).series().await
}

/// What the plans intended over this macrocycle's mesocycles, today (#689).
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn aims(store: &Store, zone: &OperatorZone) -> Result<Vec<Aim>, StoreError> {
    Aims::new(store.plans(zone))
        .on(crate::equipment::today(zone), Calendars.holidays().await)
        .await
}
