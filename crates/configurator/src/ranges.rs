//! The ranges a chart can be read over today, over this build's store (#497).

use application::ranges::CurrentRanges;
use domain::{analytical::range::Ranges, normalised::OperatorZone};

use crate::{next::NextFailure, rescheduling, store::Store};

/// The mesocycle and macrocycle today falls in, each to today, as the plans
/// now stand (#800).
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub async fn today(store: &Store, zone: &OperatorZone) -> Result<Ranges, NextFailure> {
    Ok(CurrentRanges::new(rescheduling::plans(store, zone).await?)
        .on(crate::equipment::today(zone))
        .await?)
}
