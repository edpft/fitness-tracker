//! Body weight as one time series, over this build's store (#250, #252).

use application::{
    StoreError,
    body_weight::{BodyWeight, View},
};
use domain::normalised::OperatorZone;

use crate::store::Store;

/// Every canonical weigh-in, the trend through them and which way it went,
/// and the cycles today falls in.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn view(store: &Store, zone: &OperatorZone) -> Result<View, StoreError> {
    let today = jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date();
    BodyWeight::new(store.weigh_ins(), store.plans(zone))
        .view(today)
        .await
}
