//! Estimated 1RM as a trend, over this build's store (#420).

use application::{
    StoreError,
    one_rep_max::{EstimatedOneRepMax, View},
};
use domain::{analytical::Rts, normalised::OperatorZone};

use crate::store::Store;

/// Each lift's estimates in each planned macrocycle, through today.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn view(store: &Store, zone: &OperatorZone) -> Result<View, StoreError> {
    let today = jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date();
    EstimatedOneRepMax::new(Rts, store.canonical_gym(), store.plans(zone))
        .view(today)
        .await
}
