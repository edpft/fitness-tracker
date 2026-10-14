//! Body weight as one time series, over this build's store (#250).

use application::{StoreError, body_weight::BodyWeight};
use domain::analytical::Weighed;

use crate::store::Store;

/// Every canonical weigh-in, oldest first.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn series(store: &Store) -> Result<Vec<Weighed>, StoreError> {
    BodyWeight::new(store.weigh_ins()).series().await
}
