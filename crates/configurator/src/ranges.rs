//! The ranges a chart can be read over today, over this build's store (#497).

use std::future::{Ready, ready};

use application::StoreError;
use domain::{analytical::range::Ranges, normalised::OperatorZone};
use jiff::civil::Date;

use crate::{
    next::NextFailure,
    rescheduling::{self, Plans},
    store::Store,
};

/// The mesocycle and macrocycle today falls in, each to today, as the plans
/// now stand (#800): read from the plans the walk read (#817).
pub type Current = Plans;

/// The ranges over `store`.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub(crate) async fn over(store: &Store, zone: &OperatorZone) -> Result<Current, NextFailure> {
    rescheduling::plans(store, zone).await
}

/// The ranges `today` falls in among `plans`, which the walk already read:
/// ready at once, as a future so that a page reads them as it reads
/// everything else.
pub(crate) fn on(plans: &Plans, today: Date) -> Ready<Result<Ranges, StoreError>> {
    ready(Ok(application::ranges::of(today, plans.now())))
}
