//! Estimated 1RM as a trend, and over body weight and fat-free mass, over
//! this build's store (#420, #253).

use application::{
    StoreError,
    next::Scope,
    one_rep_max::{EstimatedOneRepMax, OneRepMaxTests, View},
};
use domain::{analytical::Rts, normalised::OperatorZone};
use infrastructure::{
    SqliteCanonicalGymSessionStore, SqliteCanonicalWeighInStore, SqlitePlanStore,
    SqlitePrescribedWorkoutStore,
};

use crate::store::Spanned;
use crate::{
    graph::{page, read},
    next::NextFailure,
    rescheduling::{self, Plans},
    store::Store,
};

/// Each lift over the macrocycle in force, its mesocycles as the plans now
/// stand (#800).
pub(crate) type Estimated = EstimatedOneRepMax<
    Rts,
    Spanned<SqliteCanonicalGymSessionStore>,
    Spanned<SqliteCanonicalWeighInStore>,
    Plans,
>;

/// Each lift's 1RM tests (#517).
pub(crate) type Tests = OneRepMaxTests<
    Rts,
    Spanned<SqliteCanonicalGymSessionStore>,
    Spanned<SqlitePlanStore>,
    Spanned<SqlitePrescribedWorkoutStore>,
>;

page! {
    /// What the estimated 1RM page reads: the canonical gym sessions (#559),
    /// the canonical weigh-ins, and the plans as they now stand.
    pub struct Page {
        estimated: Estimated,
    }
}

/// The parts a run collecting what the page reads takes.
#[must_use]
pub fn collects() -> Scope {
    crate::graph::collects::<Page>()
}

/// Each lift over the macrocycle in force in `plans`.
pub(crate) fn estimated(store: &Store, plans: Plans) -> Estimated {
    EstimatedOneRepMax::new(
        Rts,
        store.adapter(SqliteCanonicalGymSessionStore::new),
        store.adapter(SqliteCanonicalWeighInStore::new),
        plans,
    )
}

/// Each lift's 1RM tests over `store`.
pub(crate) fn tests(store: &Store, zone: &OperatorZone) -> Tests {
    OneRepMaxTests::new(
        Rts,
        store.adapter(SqliteCanonicalGymSessionStore::new),
        store.adapter_with(SqlitePlanStore::new, zone.clone()),
        store.adapter_with(SqlitePrescribedWorkoutStore::new, zone.id().to_owned()),
    )
}

impl Page {
    /// The page over `store`.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn open(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        Ok(Self {
            estimated: estimated(store, rescheduling::plans(store, zone).await?).into(),
        })
    }

    /// Each lift over the macrocycle in force, through today.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn view(&self, zone: &OperatorZone) -> Result<View, StoreError> {
        read!(self.estimated.view(crate::equipment::today(zone))).await
    }
}
