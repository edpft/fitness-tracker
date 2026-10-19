//! The ranges a chart can be read over today, over this build's store (#497).

use application::{StoreError, ranges::CurrentRanges};
use domain::{analytical::range::Ranges, normalised::OperatorZone};

use crate::store::Store;

/// The mesocycle and macrocycle today falls in, each to today.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn today(store: &Store, zone: &OperatorZone) -> Result<Ranges, StoreError> {
    CurrentRanges::new(store.plans(zone))
        .on(crate::equipment::today(zone))
        .await
}
