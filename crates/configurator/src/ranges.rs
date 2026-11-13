//! The ranges a chart can be read over today, over this build's store (#497).

use application::{StoreError, ranges::CurrentRanges};
use domain::{analytical::range::Ranges, normalised::OperatorZone};

use crate::{
    next::NextFailure,
    rescheduling::{self, Plans},
    store::Store,
};

/// The mesocycle and macrocycle today falls in, each to today, as the plans
/// now stand (#800).
pub type Current = CurrentRanges<Plans>;

/// The ranges over `store`.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub(crate) async fn over(store: &Store, zone: &OperatorZone) -> Result<Current, NextFailure> {
    Ok(CurrentRanges::new(rescheduling::plans(store, zone).await?))
}

/// The ranges `current` gives today.
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub(crate) async fn today(current: &Current, zone: &OperatorZone) -> Result<Ranges, StoreError> {
    current.on(crate::equipment::today(zone)).await
}
