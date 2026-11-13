//! Estimated 1RM as a trend, and over body weight and fat-free mass, over
//! this build's store (#420, #253).

use application::{
    CanonicalLayer, PlanStore, StoreError,
    next::Scope,
    one_rep_max::{EstimatedOneRepMax, OneRepMaxTests, Tests, View},
};
use domain::{analytical::Rts, normalised::OperatorZone};

use crate::{
    next::{self, NextFailure, Reads},
    rescheduling,
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

/// Each lift over the macrocycle in force, through today, its mesocycles as
/// the plans now stand (#800).
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub async fn view(store: &Store, zone: &OperatorZone) -> Result<View, NextFailure> {
    Ok(view_over(store, rescheduling::plans(store, zone).await?, zone).await?)
}

/// Each lift over the macrocycle in force in `plans`, through today.
pub(crate) async fn view_over(
    store: &Store,
    plans: impl PlanStore + Sync,
    zone: &OperatorZone,
) -> Result<View, StoreError> {
    let today = jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date();
    EstimatedOneRepMax::new(Rts, store.canonical_gym(), store.weigh_ins(), plans)
        .view(today)
        .await
}

/// Each lift's 1RM tests, through today (#517).
///
/// # Errors
///
/// [`StoreError`] if the store cannot be read.
pub async fn tests(store: &Store, zone: &OperatorZone) -> Result<Vec<Tests>, StoreError> {
    let today = jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date();
    OneRepMaxTests::new(
        Rts,
        store.canonical_gym(),
        store.plans(zone),
        store.prescriptions(zone),
    )
    .tests(today)
    .await
}
