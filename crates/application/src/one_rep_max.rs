//! Estimated 1RM as a trend, from one test to the next (#420), and over body
//! weight and fat-free mass (#253).
//!
//! **The estimates, the ratios, the stretches, the lines and the next test
//! are `domain`'s** ([`points`], [`relative`], [`stretches`], [`next_test`]);
//! this reads the canonical gym sessions, the canonical weigh-ins and the
//! plans, and hands them over. Nothing is
//! written: the view re-derives from them exactly (§ 5).

use domain::{
    analytical::{
        OneRepMaxEstimator,
        one_rep_max::{
            Against, LIFTS, Relative, Stretch, TestRead, next_test, planned_tests, points,
            read_tests, relative, sessions_of, stretches, tests,
        },
    },
    gym::exercise::RepsExercise,
};
use jiff::civil::{Date, date};

use crate::{
    error::StoreError,
    ports::{PerformedGymSessions, PlanStore, PrescribedWorkoutStore, WeighInHistory},
};

/// Before anything the record holds.
const EARLIEST: Date = date(2000, 1, 1);

/// Each lift's stretches.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The estimator's name. A different estimator is a different series (§ 6).
    pub estimator: &'static str,
    pub lifts: Vec<Lift>,
}

/// One lift's stretches: in kilograms, and over each body mass.
#[derive(Debug, Clone, PartialEq)]
pub struct Lift {
    pub lift: RepsExercise,
    pub absolute: Vec<Stretch>,
    /// Each test, oldest first, against the one before and the start of its
    /// run.
    pub tests: Vec<TestRead>,
    /// In [`Against::ALL`]'s order.
    pub relative: Vec<(Against, Vec<Stretch<Relative>>)>,
}

/// The use case.
pub struct EstimatedOneRepMax<E, S, W, P, R> {
    estimator: E,
    sessions: S,
    weigh_ins: W,
    plans: P,
    prescribed: R,
}

impl<E, S, W, P, R> EstimatedOneRepMax<E, S, W, P, R>
where
    E: OneRepMaxEstimator + Sync,
    S: PerformedGymSessions + Sync,
    W: WeighInHistory + Sync,
    P: PlanStore + Sync,
    R: PrescribedWorkoutStore + Sync,
{
    pub const fn new(estimator: E, sessions: S, weigh_ins: W, plans: P, prescribed: R) -> Self {
        Self {
            estimator,
            sessions,
            weigh_ins,
            plans,
            prescribed,
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
        let weigh_ins = self.weigh_ins.weigh_ins().await?;
        let mut plans = Vec::new();
        for window in self.plans.windows().await? {
            if let Some(plan) = self.plans.named(window.name()).await?
                && !plans.contains(&plan)
            {
                plans.push(plan);
            }
        }
        let yesterday = today.yesterday().unwrap_or(today);
        // The prescription in force on each day a lift was performed, which
        // says whether that session was a test.
        let mut prescribed = Vec::new();
        for lift in LIFTS {
            for day in sessions_of(&sessions, lift) {
                if let Some((_, prescription)) = self.prescribed.issued_for(day).await? {
                    prescribed.push(prescription);
                }
            }
        }
        Ok(View {
            estimator: E::NAME,
            lifts: LIFTS
                .into_iter()
                .map(|lift| {
                    let planned = planned_tests(&plans, &prescribed, lift);
                    let points = points(&self.estimator, &sessions, lift, &planned);
                    let days = sessions_of(&sessions, lift);
                    let after = days.last().map_or(yesterday, |last| (*last).max(yesterday));
                    let next = next_test(&plans, after);
                    let tests = tests(&points);
                    Lift {
                        lift,
                        absolute: stretches(&points, &tests, &days, next),
                        tests: read_tests(&points),
                        relative: Against::ALL
                            .into_iter()
                            .map(|against| {
                                let relative = relative(
                                    &self.estimator,
                                    &sessions,
                                    lift,
                                    &planned,
                                    &weigh_ins,
                                    against,
                                );
                                (against, stretches(&relative, &tests, &days, next))
                            })
                            .collect(),
                    }
                })
                .collect(),
        })
    }
}
