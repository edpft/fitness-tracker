//! Estimated 1RM as a trend, from one test to the next (#420).
//!
//! **The estimates, the stretches, the lines and the next test are
//! `domain`'s** ([`points`], [`stretches`], [`next_test`]); this reads the
//! canonical gym sessions and the plans, and hands them over. Nothing is
//! written: the view re-derives from them exactly (§ 5).

use domain::{
    analytical::{
        OneRepMaxEstimator,
        one_rep_max::{LIFTS, Stretch, next_test, points, sessions_of, stretches},
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

/// Each lift's stretches.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The estimator's name. A different estimator is a different series (§ 6).
    pub estimator: &'static str,
    pub lifts: Vec<(RepsExercise, Vec<Stretch>)>,
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

    /// Every lift's stretches through `today`, the open one projected to the
    /// next planned test.
    ///
    /// **The planned sessions are counted from the later of the last one
    /// performed and yesterday**, so a programmed day already gone by without
    /// a session is not one still to come.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn view(&self, today: Date) -> Result<View, StoreError> {
        let sessions = self.sessions.between(EARLIEST, today).await?;
        let mut plans = Vec::new();
        for window in self.plans.windows().await? {
            if let Some(plan) = self.plans.named(window.name()).await?
                && !plans.contains(&plan)
            {
                plans.push(plan);
            }
        }
        let yesterday = today.yesterday().unwrap_or(today);
        Ok(View {
            estimator: E::NAME,
            lifts: LIFTS
                .into_iter()
                .map(|lift| {
                    let points = points(&self.estimator, &sessions, lift);
                    let days = sessions_of(&sessions, lift);
                    let after = days.last().map_or(yesterday, |last| (*last).max(yesterday));
                    let next = next_test(&plans, after);
                    (lift, stretches(&points, &days, next))
                })
                .collect(),
        })
    }
}
