//! Estimated 1RM over the macrocycle, by one line through every session's
//! estimate (#420, #688), and over body weight and fat-free mass (#253).
//!
//! **The estimates, the ratios, the line and the next test are `domain`'s**
//! ([`points`], [`relative`], [`trend`], [`next_test`]); this reads the
//! canonical gym sessions, the canonical weigh-ins and the plans, and hands
//! them over. Nothing is written: the view re-derives from them exactly (§ 5).

use domain::{
    analytical::{
        OneRepMaxEstimator,
        one_rep_max::{
            Against, LIFTS, Point, Relative, Trend, next_test, points, relative, sessions_of, trend,
        },
    },
    gym::exercise::RepsExercise,
    plan::{Mesocycles, Occupies},
};
use jiff::civil::{Date, date};

use crate::{
    error::StoreError,
    ports::{PerformedGymSessions, PlanStore, WeighInHistory},
};

/// Before anything the record holds.
const EARLIEST: Date = date(2000, 1, 1);

/// Each lift over the macrocycle in force.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The estimator's name. A different estimator is a different series (§ 6).
    pub estimator: &'static str,
    /// The day it was read through.
    pub today: Date,
    /// The first day of the plan in force; `None` where none is, and then no
    /// lift is read.
    pub macrocycle: Option<Date>,
    /// The first day of each of its gym mesocycles, oldest first, so a chart
    /// can mark where each starts.
    pub mesocycles: Vec<Date>,
    pub lifts: Vec<Lift>,
}

/// One lift: in kilograms, and over each body mass.
#[derive(Debug, Clone, PartialEq)]
pub struct Lift {
    pub lift: RepsExercise,
    pub absolute: Trend<Point>,
    /// In [`Against::ALL`]'s order.
    pub relative: Vec<(Against, Trend<Relative>)>,
}

/// The use case.
pub struct EstimatedOneRepMax<E, S, W, P> {
    estimator: E,
    sessions: S,
    weigh_ins: W,
    plans: P,
}

impl<E, S, W, P> EstimatedOneRepMax<E, S, W, P>
where
    E: OneRepMaxEstimator + Sync,
    S: PerformedGymSessions + Sync,
    W: WeighInHistory + Sync,
    P: PlanStore + Sync,
{
    pub const fn new(estimator: E, sessions: S, weigh_ins: W, plans: P) -> Self {
        Self {
            estimator,
            sessions,
            weigh_ins,
            plans,
        }
    }

    /// Every lift over the macrocycle in force on `today`, carried on to the
    /// next planned test.
    ///
    /// **The macrocycle is the plan in force**, as the pages' ranges read it
    /// (#443): from its first day.
    ///
    /// **The next test is counted from the later of the last session performed
    /// and yesterday**, so a programmed day already gone by without a session
    /// is not one still to come.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn view(&self, today: Date) -> Result<View, StoreError> {
        let mut plans = Vec::new();
        for window in self.plans.windows().await? {
            if let Some(plan) = self.plans.named(window.name()).await?
                && !plans.contains(&plan)
            {
                plans.push(plan);
            }
        }
        let Some(plan) = plans.iter().find(|plan| plan.span().covers(today)) else {
            return Ok(View {
                estimator: E::NAME,
                today,
                macrocycle: None,
                mesocycles: Vec::new(),
                lifts: Vec::new(),
            });
        };
        let from = plan.span().start();
        let mut mesocycles: Vec<Date> = plan
            .gym()
            .into_iter()
            .flat_map(Mesocycles::mesocycles)
            .map(|mesocycle| Occupies::span(mesocycle).start())
            .collect();
        mesocycles.sort_unstable();
        let sessions = self.sessions.between(EARLIEST, today).await?;
        let weigh_ins = self.weigh_ins.weigh_ins().await?;
        let yesterday = today.yesterday().unwrap_or(today);
        Ok(View {
            estimator: E::NAME,
            today,
            macrocycle: Some(from),
            mesocycles,
            lifts: LIFTS
                .into_iter()
                .map(|lift| {
                    // Every session's estimate, a test's included: the tests
                    // are their own record (#706).
                    let points = points(&self.estimator, &sessions, lift, &[]);
                    let after = sessions_of(&sessions, lift)
                        .last()
                        .map_or(yesterday, |last| (*last).max(yesterday));
                    let next = next_test(&plans, after);
                    Lift {
                        lift,
                        absolute: trend(&points, from, next),
                        relative: Against::ALL
                            .into_iter()
                            .map(|against| {
                                let relative = relative(
                                    &self.estimator,
                                    &sessions,
                                    lift,
                                    &[],
                                    &weigh_ins,
                                    against,
                                );
                                (against, trend(&relative, from, next))
                            })
                            .collect(),
                    }
                })
                .collect(),
        })
    }
}
