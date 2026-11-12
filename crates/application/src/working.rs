//! Is what I'm doing working?, for the macrocycle in force (#517), as
//! Progress reads it (#769).
//!
//! The operator, 2026-11-11: *"has this macrocycle improved my fitness so
//! far?"* **Two kinds of reading answer it.** Performance is what a test
//! gives: the latest FTP test and the latest 1RM test, each against the
//! macrocycle's entry test. Response is whether his body is answering the
//! training as expected: resting heart rate, overnight HRV and stress, each
//! against the expected result of the training done.
//!
//! **Progress reads it by discipline and adds the body** (operator,
//! 2026-11-12): each discipline's test, then its estimate since the start, and
//! body weight against the plan's aims with fat and fat-free mass beside it.
//! *"Progress must be measured against plan or expectation, and the plan is
//! the training in the macrocycle."*
//!
//! **The readings are `domain`'s** ([`since_entry`], [`response`]); this reads
//! the plans, the readings, the register and the training done, and hands
//! them over. Nothing is written: the view re-derives from them exactly
//! (§ 5).
//!
//! **One watch's days**, the one that recorded the latest of each reading: a
//! watch's figures are never read against another's (§ 6), and the line the
//! expected result is read off is drawn through that watch's weeks alone.

use std::collections::BTreeMap;

use domain::{
    analytical::{
        body_weight::{Aim, Series, Weight, against_aims},
        composition::{ComponentSeries, Unit},
        ftp_test, heart_rate_at_power, one_rep_max,
        per_device::{DeviceSeries, SentBy, per_device},
        response::{Response, response},
        since_entry::{SinceEntry, since_entry},
        since_start::{SinceStart, since_start},
        stress,
        training_done::Carried,
    },
    body::Component,
    device::Measurand,
    gym::exercise::RepsExercise,
    plan::Span,
    planner::Discipline,
};
use jiff::civil::Date;

use crate::{
    devices::DeviceRegister,
    error::StoreError,
    one_rep_max::{Lift, Tests},
    ports::{
        OvernightHrvHistory, PlanStore, RecordedDevices, RestingHeartRateHistory, StressHistory,
        TrainingDone, WearRecord,
    },
};

/// What the page says on a day.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    /// The day it was read through.
    pub today: Date,
    /// `None` on a day no plan is in force.
    pub macrocycle: Option<Macrocycle>,
}

/// The macrocycle in force, so far.
#[derive(Debug, Clone, PartialEq)]
pub struct Macrocycle {
    /// Its intended start: the first day of the plan.
    pub began: Date,
    /// The days of each of its mesocycles, oldest first.
    pub mesocycles: Vec<Span>,
    /// The first day of the macrocycle before it, where there was one.
    pub last_began: Option<Date>,
    /// Each test with its estimate: FTP first, then each lift.
    pub performance: Vec<Performance>,
    pub body: Body,
    pub responses: Vec<Responding>,
}

/// Body weight and body composition, read together (#769).
#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    /// `None` where fewer than two trend days fall in the macrocycle.
    pub weight: Option<Weight>,
    /// In kilograms, by the method whose trend is the latest.
    pub fat: Option<SinceStart>,
    pub fat_free: Option<SinceStart>,
}

/// What a test measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tested {
    /// In watts.
    Ftp,
    /// In grams.
    OneRepMax(RepsExercise),
}

impl Tested {
    /// The discipline whose test it is.
    pub const fn discipline(self) -> Discipline {
        match self {
            Self::Ftp => Discipline::Cycling,
            Self::OneRepMax(_) => Discipline::Gym,
        }
    }
}

/// One test's reading over the macrocycle, and the estimate read beside it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Performance {
    pub tested: Tested,
    /// `None` where the macrocycle holds no test of it yet.
    pub since: Option<SinceEntry>,
    /// Heart rate at 140 W beside FTP, the estimated 1RM beside a lift's
    /// test, each since the start; `None` where its line has too few days in
    /// the macrocycle.
    pub estimated: Option<SinceStart>,
}

/// What the other pages' use cases have already read, which Progress reads
/// beside the response readings.
pub struct Readings<'a> {
    /// Every FTP test.
    pub ftp: &'a [ftp_test::Test],
    /// Every lift's 1RM tests.
    pub strength: &'a [Tests],
    /// Each lift's estimated 1RM over the macrocycle in force.
    pub estimated: &'a [Lift],
    /// Heart rate at 140 W, one series per device.
    pub at_power: &'a [heart_rate_at_power::Trended],
    pub weight: &'a Series,
    pub aims: &'a [Aim],
    pub composition: &'a [ComponentSeries],
}

/// A reading that answers training.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answering {
    RestingHeartRate,
    OvernightHrv,
    Stress,
}

/// One response reading over the macrocycle.
#[derive(Debug, Clone, PartialEq)]
pub struct Responding {
    pub reading: Answering,
    /// The watch whose days these are; `None` where no watch has any.
    pub sent_by: Option<SentBy>,
    /// `None` where the two weeks before the start hold too few days.
    pub response: Option<Response>,
}

/// The use case.
pub struct IsItWorking<N, D, S, T, R, W, P> {
    nights: N,
    days: D,
    stress: S,
    training: T,
    devices: DeviceRegister<R, W>,
    plans: P,
}

impl<N, D, S, T, R, W, P> IsItWorking<N, D, S, T, R, W, P>
where
    N: OvernightHrvHistory + Sync,
    D: RestingHeartRateHistory + Sync,
    S: StressHistory + Sync,
    T: TrainingDone + Sync,
    R: RecordedDevices + Sync,
    W: WearRecord + Sync,
    P: PlanStore + Sync,
{
    pub const fn new(
        nights: N,
        days: D,
        stress: S,
        training: T,
        devices: DeviceRegister<R, W>,
        plans: P,
    ) -> Self {
        Self {
            nights,
            days,
            stress,
            training,
            devices,
            plans,
        }
    }

    /// The macrocycle in force on `today`, read through `today`.
    ///
    /// `readings` is what the other pages' use cases read.
    ///
    /// **The macrocycle is the plan in force**, as the pages' ranges read it
    /// (#443), and its start is the plan's first day whatever was performed
    /// in its first weeks (operator, 2026-11-11).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn view(&self, today: Date, readings: &Readings<'_>) -> Result<View, StoreError> {
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
                today,
                macrocycle: None,
            });
        };
        let began = plan.span().start();
        let last_began = plans
            .iter()
            .map(|plan| plan.span().start())
            .filter(|start| *start < began)
            .max();

        let register = self.devices.register().await?;
        let carried = Carried::through(&self.training.sessions().await?, today);
        let read = |reading: Answering, series: Option<(SentBy, BTreeMap<Date, f64>)>| {
            let (sent_by, days) = series.unzip();
            Responding {
                reading,
                sent_by,
                response: days.and_then(|days| response(&days, &carried, began, today)),
            }
        };
        let resting = latest(
            per_device(self.days.days().await?, Measurand::HeartRate, &register),
            today,
        )
        .map(|series| {
            let days = series
                .readings
                .iter()
                .map(|reading| (reading.on, f64::from(reading.value.as_u32())))
                .collect();
            (series.sent_by, days)
        });
        let hrv = latest(
            per_device(self.nights.nights().await?, Measurand::HeartRate, &register),
            today,
        )
        .map(|series| {
            let days = series
                .readings
                .iter()
                .map(|reading| (reading.on, f64::from(reading.value.as_milliseconds())))
                .collect();
            (series.sent_by, days)
        });
        let stress = latest(
            per_device(self.stress.days().await?, Measurand::HeartRate, &register),
            today,
        )
        .map(|series| (series.sent_by, stress::whole_days(&series.readings)));

        Ok(View {
            today,
            macrocycle: Some(Macrocycle {
                began,
                mesocycles: plan.mesocycle_spans(),
                last_began,
                performance: performance(readings, began, today),
                body: Body {
                    weight: against_aims(&readings.weight.trend, readings.aims, began, today),
                    fat: composed(readings.composition, Component::Fat, began, today),
                    fat_free: composed(readings.composition, Component::FatFree, began, today),
                },
                responses: vec![
                    read(Answering::RestingHeartRate, resting),
                    read(Answering::OvernightHrv, hrv),
                    read(Answering::Stress, stress),
                ],
            }),
        })
    }
}

/// Each test's reading over the macrocycle that `began` on a day, with its
/// estimate: the FTP test, then each lift's 1RM test.
fn performance(read: &Readings<'_>, began: Date, today: Date) -> Vec<Performance> {
    let watts: Vec<(Date, u32)> = read
        .ftp
        .iter()
        .map(|test| (test.on(), test.ftp.watts().as_u32()))
        .collect();
    let estimated = |lift: RepsExercise| {
        let trend = &read.estimated.iter().find(|one| one.lift == lift)?.absolute;
        let change = trend.change?;
        let now = trend.now()?;
        Some(SinceStart {
            from: now - change.by,
            now,
            margin: Some(change.margin),
        })
    };
    let lifts = read.strength.iter().map(|lift| Performance {
        tested: Tested::OneRepMax(lift.lift),
        since: since_entry(&lift.tests, began, today, one_rep_max::noise),
        estimated: estimated(lift.lift),
    });
    let estimated_only = read
        .estimated
        .iter()
        .filter(|one| !read.strength.iter().any(|tests| tests.lift == one.lift))
        .map(|one| Performance {
            tested: Tested::OneRepMax(one.lift),
            since: None,
            estimated: estimated(one.lift),
        });
    std::iter::once(Performance {
        tested: Tested::Ftp,
        since: since_entry(&watts, began, today, ftp_test::noise),
        estimated: at_power(read.at_power, began, today),
    })
    .chain(lifts)
    .chain(estimated_only)
    .collect()
}

/// Heart rate at 140 W since the start, on the line of the device that rode
/// last: a device's line is never read against another's (§ 6).
fn at_power(
    trended: &[heart_rate_at_power::Trended],
    began: Date,
    today: Date,
) -> Option<SinceStart> {
    let latest = trended
        .iter()
        .filter_map(|one| Some((one.rides.last()?.on, one)))
        .max_by_key(|(last, _)| *last)?
        .1;
    since_start(
        latest
            .line
            .iter()
            .flatten()
            .map(|day| (day.on, day.bpm, day.margin)),
        began,
        today,
    )
}

/// A component's mass since the start, by the method whose trend is the
/// latest, as its page's summary reads it.
fn composed(
    components: &[ComponentSeries],
    component: Component,
    began: Date,
    today: Date,
) -> Option<SinceStart> {
    let series = components
        .iter()
        .find(|one| one.component == component && one.unit == Unit::Kilograms)?;
    let method = series
        .series
        .iter()
        .filter_map(|one| Some((one.trend.iter().flatten().next_back()?.on, one)))
        .max_by_key(|(last, _)| *last)?
        .1;
    since_start(
        method
            .trend
            .iter()
            .flatten()
            .map(|day| (day.on, day.value, day.margin)),
        began,
        today,
    )
}

/// The watch that recorded the latest day on or before `today`: the one in
/// use. A day no watch can be placed on is not one's.
fn latest<T>(series: Vec<DeviceSeries<T>>, today: Date) -> Option<DeviceSeries<T>> {
    series
        .into_iter()
        .filter(|series| matches!(series.sent_by, SentBy::Device(_)))
        .filter_map(|series| {
            let last = series
                .readings
                .iter()
                .map(|reading| reading.on)
                .filter(|on| *on <= today)
                .max()?;
            Some((last, series))
        })
        .max_by_key(|(last, _)| *last)
        .map(|(_, series)| series)
}
