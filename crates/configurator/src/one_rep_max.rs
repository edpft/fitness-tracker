//! Estimated 1RM as a trend, and over body weight and fat-free mass, over
//! this build's store (#420, #253).

use application::{
    CanonicalLayer, StoreError,
    next::Scope,
    one_rep_max::{EstimatedOneRepMax, View},
};
use domain::{analytical::Rts, normalised::OperatorZone};

use crate::{
    next::{self, Reads},
    store::Store,
    wiring::WiringError,
};

/// What the estimated 1RM charts read: the canonical gym sessions (#559) and
/// the canonical weigh-ins.
///
/// # Errors
///
/// [`WiringError`] if a name read is not a stream's.
pub fn collects() -> Result<Scope, WiringError> {
    next::scope([
        Reads::Canonical(CanonicalLayer::Gym),
        Reads::Canonical(CanonicalLayer::WeighIns),
    ])
}

/// Each lift's stretches between tests, through today.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn view(store: &Store, zone: &OperatorZone) -> Result<View, StoreError> {
    let today = jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date();
    EstimatedOneRepMax::new(
        Rts,
        store.canonical_gym(),
        store.weigh_ins(),
        store.plans(zone),
    )
    .view(today)
    .await
}
