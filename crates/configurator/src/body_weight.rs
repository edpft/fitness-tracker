//! Body weight as one time series, over this build's store (#250).

use application::{CanonicalLayer, StoreError, body_weight::BodyWeight, next::Scope};
use domain::analytical::body_weight::Series;

use crate::{
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

/// Every canonical weigh-in, the trend through them and which way it went.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn series(store: &Store) -> Result<Series, StoreError> {
    BodyWeight::new(store.weigh_ins()).series().await
}
