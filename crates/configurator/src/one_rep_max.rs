//! Estimated 1RM as a trend, and over body weight and fat-free mass, over
//! this build's store (#420, #253).

use application::one_rep_max::{EstimatedOneRepMax, OneRepMaxTests};
use domain::{analytical::Rts, normalised::OperatorZone};
use infrastructure::{SqlitePerformances, SqlitePlanStore, SqlitePrescribedWorkoutStore};

use crate::store::{Spanned, Store};

/// Each lift over the macrocycle in force, its mesocycles as the plans now
/// stand (#800).
pub(crate) type Estimated = EstimatedOneRepMax<Rts, Spanned<SqlitePerformances>>;

/// Each lift's 1RM tests and estimates, read once for both (#817).
pub(crate) type Strength = application::one_rep_max::Strength<
    Rts,
    Spanned<SqlitePerformances>,
    Spanned<SqlitePlanStore>,
    Spanned<SqlitePrescribedWorkoutStore>,
>;

/// Each lift's 1RM tests (#517).
pub(crate) type Tests = OneRepMaxTests<
    Rts,
    Spanned<SqlitePerformances>,
    Spanned<SqlitePlanStore>,
    Spanned<SqlitePrescribedWorkoutStore>,
>;

/// Each lift over the macrocycle in force.
pub(crate) fn estimated(store: &Store) -> Estimated {
    EstimatedOneRepMax::new(Rts, store.adapter(SqlitePerformances::new))
}

/// Each lift's 1RM tests over `store`.
pub(crate) fn tests(store: &Store, zone: &OperatorZone) -> Tests {
    OneRepMaxTests::new(
        Rts,
        store.adapter(SqlitePerformances::new),
        store.adapter_with(SqlitePlanStore::new, zone.clone()),
        store.adapter_with(SqlitePrescribedWorkoutStore::new, zone.id().to_owned()),
    )
}
