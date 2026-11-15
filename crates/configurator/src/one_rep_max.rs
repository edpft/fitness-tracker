//! Estimated 1RM as a trend, and over body weight and fat-free mass, over
//! this build's store (#420, #253).

use application::{
    PlanStore as _, StoreError,
    next::Scope,
    one_rep_max::{EstimatedOneRepMax, OneRepMaxTests, View},
};
use domain::{analytical::Rts, normalised::OperatorZone};
use infrastructure::{SqlitePerformances, SqlitePlanStore, SqlitePrescribedWorkoutStore};

use crate::clock::Today;
use crate::store::Spanned;
use crate::{
    body_weight,
    graph::{page, read},
    next::NextFailure,
    rescheduling::{self, Plans},
    store::Store,
};

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

page! {
    /// What the estimated 1RM page reads: the lifts' performances (#559, #817),
    /// the canonical weigh-ins, and the plans as they now stand.
    pub struct Page {
        estimated: Estimated,
        weight: body_weight::Series,
        plans: Plans,
        today: Today,
    }
}

/// The parts a run collecting what the page reads takes.
#[must_use]
pub fn collects() -> Scope {
    crate::graph::scope(&reads())
}

/// What the page reads, and so what its kept drawing is drawn from (#805).
#[must_use]
pub fn reads() -> application::graph::Reads {
    <Page as application::graph::Input>::reads()
}

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

impl Page {
    /// The page over `store`.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn open(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        Ok(Self {
            estimated: estimated(store).into(),
            weight: body_weight::series(store).into(),
            plans: rescheduling::plans(store, zone).await?.into(),
            today: Today::new(zone.clone()).into(),
        })
    }

    /// Each lift over the macrocycle in force, through today.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn view(&self) -> Result<View, StoreError> {
        let plans = read!(self.plans.plans()).await?;
        let weigh_ins = read!(self.weight.weigh_ins()).await?;
        read!(self.estimated.view(self.today.date(), &plans, &weigh_ins)).await
    }
}
