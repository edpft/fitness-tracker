//! Body weight as one time series, over this build's store (#250).

use application::{StoreError, body_weight::BodyWeight};
use domain::analytical::body_weight::Series;

use crate::store::Store;

/// Every canonical weigh-in, the trend through them and which way it went.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn series(store: &Store) -> Result<Series, StoreError> {
    BodyWeight::new(store.weigh_ins()).series().await
}
