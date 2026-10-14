//! Estimated 1RM as a trend, one planned macrocycle at a time (#420).
//!
//! **The estimates, the macrocycles and the lines are `domain`'s**
//! ([`points`], [`by_macrocycle`]); this reads the canonical gym sessions and
//! the planned macrocycles, and hands them over. Nothing is written: the view
//! re-derives from them exactly (§ 5).

use domain::{
    analytical::{
        OneRepMaxEstimator,
        one_rep_max::{LIFTS, Planned, by_macrocycle, points},
    },
    gym::exercise::RepsExercise,
};
use jiff::civil::{Date, date};

use crate::{
    error::StoreError,
    ports::{PerformedGymSessions, PlanStore},
};

/// Before anything the record holds.
const EARLIEST: Date = date(2000, 1, 1);

/// Each lift in each planned macrocycle.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The estimator's name. A different estimator is a different series (§ 6).
    pub estimator: &'static str,
    pub lifts: Vec<(RepsExercise, Vec<Planned>)>,
}

/// The use case.
pub struct EstimatedOneRepMax<E, S, P> {
    estimator: E,
    sessions: S,
    plans: P,
}

impl<E, S, P> EstimatedOneRepMax<E, S, P>
where
    E: OneRepMaxEstimator + Sync,
    S: PerformedGymSessions + Sync,
    P: PlanStore + Sync,
{
    pub const fn new(estimator: E, sessions: S, plans: P) -> Self {
        Self {
            estimator,
            sessions,
            plans,
        }
    }

    /// Every lift's estimates through `through`, in each planned macrocycle.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn view(&self, through: Date) -> Result<View, StoreError> {
        let sessions = self.sessions.between(EARLIEST, through).await?;
        let plans = self.plans.windows().await?;
        Ok(View {
            estimator: E::NAME,
            lifts: LIFTS
                .into_iter()
                .map(|lift| {
                    let points = points(&self.estimator, &sessions, lift);
                    (lift, by_macrocycle(&points, &plans))
                })
                .collect(),
        })
    }
}
