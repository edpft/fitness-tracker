//! Estimated 1RM over the macrocycle, by one line through every session's
//! estimate (#420, #688), and over body weight and fat-free mass (#253).
//!
//! **The estimates, the ratios, the line and the next test are `domain`'s**
//! ([`points`], [`relative`], [`trend`], [`next_test`]); this reads the
//! canonical gym sessions, the canonical weigh-ins and the plans, and hands
//! them over. Nothing is written: the view re-derives from them exactly (§ 5).

use domain::{
    analytical::{
        OneRepMaxEstimator, Weighed,
        one_rep_max::{
            Against, LIFTS, Point, Reading, Relative, Trend, next_test, planned_tests, points,
            relative, sessions_of, trend,
        },
    },
    canonical::Occurred,
    gym::{
        Performance,
        exercise::{Exercise, RepsExercise},
    },
    plan::{Mesocycles, Occupies, Plan},
};
use jiff::civil::Date;

use crate::{
    error::StoreError,
    ports::{Performances, PlanStore, PrescribedWorkoutStore},
};

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
pub struct EstimatedOneRepMax<E, S> {
    estimator: E,
    sessions: S,
}

crate::graph::through!(EstimatedOneRepMax<E, S>);

impl<E, S> EstimatedOneRepMax<E, S>
where
    E: OneRepMaxEstimator + Sync,
    S: Performances + Sync,
{
    pub const fn new(estimator: E, sessions: S) -> Self {
        Self {
            estimator,
            sessions,
        }
    }

    /// Every lift over the macrocycle in force on `today` among `plans`, as
    /// they now stand (#800), carried on to the next planned test; each
    /// relative to the canonical weigh-ins `weigh_ins`.
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
    pub async fn view(
        &self,
        today: Date,
        plans: &[Plan],
        weigh_ins: &[Weighed],
    ) -> Result<View, StoreError> {
        let Some(plan) = in_force(plans, today) else {
            return Ok(nothing::<E>(today));
        };
        // From the macrocycle's first day: no point before it is drawn, and
        // the next test is counted from the last session only where that is
        // today.
        let sessions = self.sessions.of(&LIFTS, plan.span().start(), today).await?;
        Ok(self.drawn(today, plans, plan, &sessions, weigh_ins))
    }

    /// The view of `plan`, the one in force on `today`, from the performances
    /// of the lifts since its first day.
    fn drawn(
        &self,
        today: Date,
        plans: &[Plan],
        plan: &Plan,
        sessions: &[Performance],
        weigh_ins: &[Weighed],
    ) -> View {
        let from = plan.span().start();
        let mut mesocycles: Vec<Date> = plan
            .gym()
            .into_iter()
            .flat_map(Mesocycles::mesocycles)
            .map(|mesocycle| Occupies::span(mesocycle).start())
            .collect();
        mesocycles.sort_unstable();
        let yesterday = today.yesterday().unwrap_or(today);
        View {
            estimator: E::NAME,
            today,
            macrocycle: Some(from),
            mesocycles,
            lifts: LIFTS
                .into_iter()
                .map(|lift| {
                    // Every session's estimate, a test's included: the tests
                    // are their own record (#706).
                    let points = points(&self.estimator, sessions, lift, &[]);
                    let after = sessions_of(sessions, lift)
                        .last()
                        .map_or(yesterday, |last| (*last).max(yesterday));
                    let next = next_test(plans, after);
                    Lift {
                        lift,
                        absolute: trend(&points, from, next),
                        relative: Against::ALL
                            .into_iter()
                            .map(|against| {
                                let relative = relative(
                                    &self.estimator,
                                    sessions,
                                    lift,
                                    &[],
                                    weigh_ins,
                                    against,
                                );
                                (against, trend(&relative, from, next))
                            })
                            .collect(),
                    }
                })
                .collect(),
        }
    }
}

/// One lift's 1RM tests, oldest first, each as the day it was performed on
/// and its max in grams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tests {
    pub lift: RepsExercise,
    pub tests: Vec<(Date, u32)>,
}

/// The 1RM tests each lift's record holds (#517).
///
/// **A session is a test where its prescription names it as one**
/// ([`planned_tests`]), read from what was performed: the plan's own table
/// does not name a mesocycle's exit test, and dated autumn 2026's entry test
/// eleven days before it was done.
///
/// **Only tests of one rep**: tests of different n are never compared with
/// each other (#706).
pub struct OneRepMaxTests<E, S, P, R> {
    estimator: E,
    sessions: S,
    plans: P,
    prescribed: R,
}

crate::graph::through!(OneRepMaxTests<E, S, P, R>);

impl<E, S, P, R> OneRepMaxTests<E, S, P, R>
where
    E: OneRepMaxEstimator + Sync,
    S: Performances + Sync,
    P: PlanStore + Sync,
    R: PrescribedWorkoutStore + Sync,
{
    pub const fn new(estimator: E, sessions: S, plans: P, prescribed: R) -> Self {
        Self {
            estimator,
            sessions,
            plans,
            prescribed,
        }
    }

    /// Every lift's 1RM tests through `today`.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn tests(&self, today: Date) -> Result<Vec<Tests>, StoreError> {
        Ok(self.read(Date::MIN, today).await?.0)
    }

    /// Every lift's 1RM tests between the two days, both included, and the
    /// performances they were read from.
    ///
    /// **No earlier than a lift's first mesocycle**: a test is a
    /// prescription's, and no lift has one before that, so no day before it is
    /// read or looked up.
    async fn read(
        &self,
        from: Date,
        to: Date,
    ) -> Result<(Vec<Tests>, Vec<Performance>), StoreError> {
        let plans = self.plans.plans().await?;
        let Some(first) = first_tested(&plans, to) else {
            return Ok((untested(), Vec::new()));
        };
        let sessions = self.sessions.of(&LIFTS, from.max(first), to).await?;
        Ok((self.drawn(&plans, &sessions).await?, sessions))
    }

    /// Each lift's tests among `sessions`, the performances of the lifts.
    async fn drawn(
        &self,
        plans: &[Plan],
        sessions: &[Performance],
    ) -> Result<Vec<Tests>, StoreError> {
        let mut read = Vec::new();
        for lift in LIFTS {
            // The prescription in force on each day the lift was performed,
            // which says whether that session was a test.
            let mut prescribed = Vec::new();
            for day in sessions_of(sessions, lift) {
                if let Some((_, prescription)) = self.prescribed.issued_for(day).await? {
                    prescribed.push(prescription);
                }
            }
            let planned: Vec<_> = planned_tests(plans, &prescribed, lift)
                .into_iter()
                .filter(|(_, reps)| reps.as_u32() == 1)
                .collect();
            let tests = points(&self.estimator, sessions, lift, &planned)
                .into_iter()
                .filter(|point| matches!(point.reading, Reading::Tested { .. }))
                .filter_map(|point| {
                    Some((
                        point.on,
                        u32::try_from(point.estimate.one_rep_max.as_grams()).ok()?,
                    ))
                })
                .collect();
            read.push(Tests { lift, tests });
        }
        Ok(read)
    }
}

/// Each lift's 1RM tests and its estimates over the macrocycle, from one read
/// of the lifts' performances (#817): Progress shows both.
///
/// **Each keeps its own plans.** The estimates are over the plan as it now
/// stands (#800) and the tests are read against the plan as written, so the
/// two are different readings and both are read.
pub struct Strength<E, S, T, R> {
    estimated: EstimatedOneRepMax<E, S>,
    tests: OneRepMaxTests<E, S, T, R>,
}

crate::graph::through!(Strength<E, S, T, R>);

impl<E, S, T, R> Strength<E, S, T, R>
where
    E: OneRepMaxEstimator + Sync,
    S: Performances + Sync,
    T: PlanStore + Sync,
    R: PrescribedWorkoutStore + Sync,
{
    pub const fn new(
        estimated: EstimatedOneRepMax<E, S>,
        tests: OneRepMaxTests<E, S, T, R>,
    ) -> Self {
        Self { estimated, tests }
    }

    /// Every lift's 1RM tests through `today`, and every lift over the
    /// macrocycle in force on it among `walked`, the plans as they now stand:
    /// what [`OneRepMaxTests::tests`] and [`EstimatedOneRepMax::view`] each
    /// give, the performances read once, from the earlier of the days each
    /// reads from.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn read(
        &self,
        today: Date,
        walked: &[Plan],
        weigh_ins: &[Weighed],
    ) -> Result<(Vec<Tests>, View), StoreError> {
        let written = self.tests.plans.plans().await?;
        let first = first_tested(&written, today);
        let plan = in_force(walked, today);
        let from = [first, plan.map(|plan| plan.span().start())]
            .into_iter()
            .flatten()
            .min();
        let sessions = match from {
            Some(from) => self.estimated.sessions.of(&LIFTS, from, today).await?,
            None => Vec::new(),
        };
        let tests = match first {
            Some(first) => self.tests.drawn(&written, &since(&sessions, first)).await?,
            None => untested(),
        };
        let view = plan.map_or_else(
            || nothing::<E>(today),
            |plan| {
                let sessions = since(&sessions, plan.span().start());
                self.estimated
                    .drawn(today, walked, plan, &sessions, weigh_ins)
            },
        );
        Ok((tests, view))
    }
}

/// The plan in force on `today`.
fn in_force(plans: &[Plan], today: Date) -> Option<&Plan> {
    plans.iter().find(|plan| plan.span().covers(today))
}

/// The view where no plan is in force: no lift is read.
const fn nothing<E: OneRepMaxEstimator>(today: Date) -> View {
    View {
        estimator: E::NAME,
        today,
        macrocycle: None,
        mesocycles: Vec::new(),
        lifts: Vec::new(),
    }
}

/// The first day of the first mesocycle whose primary is one of the lifts,
/// where that is no later than `to`.
///
/// **No earlier**: a test is a prescription's, and no lift has one before
/// that, so no day before it is read or looked up.
fn first_tested(plans: &[Plan], to: Date) -> Option<Date> {
    plans
        .iter()
        .filter_map(Plan::gym)
        .flat_map(Mesocycles::mesocycles)
        .filter(|mesocycle| {
            LIFTS
                .iter()
                .any(|lift| mesocycle.primary_exercise() == Exercise::Reps(*lift))
        })
        .map(|mesocycle| Occupies::span(mesocycle).start())
        .min()
        .filter(|first| *first <= to)
}

/// Every lift, untested.
fn untested() -> Vec<Tests> {
    LIFTS
        .into_iter()
        .map(|lift| Tests {
            lift,
            tests: Vec::new(),
        })
        .collect()
}

/// The performances on or after `from`.
fn since(sessions: &[Performance], from: Date) -> Vec<Performance> {
    sessions
        .iter()
        .filter(|performance| performance.on() >= from)
        .cloned()
        .collect()
}

/// The gym sessions that tested a lift: each performance of a lift on a day
/// its test was read from.
impl<E, S, P, R> crate::history::Tested for OneRepMaxTests<E, S, P, R>
where
    E: OneRepMaxEstimator + Sync,
    S: Performances + Sync,
    P: PlanStore + Sync,
    R: PrescribedWorkoutStore + Sync,
{
    async fn between(
        &self,
        from: Date,
        to: Date,
    ) -> Result<Vec<(Occurred, RepsExercise)>, StoreError> {
        let (tests, performances) = self.read(from, to).await?;
        Ok(tests
            .iter()
            .flat_map(|lift| {
                lift.tests.iter().flat_map(|(on, _)| {
                    performances
                        .iter()
                        .filter(|performance| {
                            performance.exercise == lift.lift && performance.on() == *on
                        })
                        .map(|performance| (performance.occurred.clone(), lift.lift))
                })
            })
            .collect())
    }
}
