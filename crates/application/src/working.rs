//! Is what I'm doing working?, for the macrocycle in force (#517).
//!
//! The operator, 2026-11-11: *"has this macrocycle improved my fitness so
//! far?"* **Two kinds of reading answer it.** Performance is what a test
//! gives: the latest FTP test and the latest 1RM test, each against the
//! macrocycle's entry test. Response is whether his body is answering the
//! training as expected: resting heart rate, overnight HRV and stress, each
//! against the expected result of the training done.
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
        ftp_test, one_rep_max,
        per_device::{DeviceSeries, SentBy, per_device},
        response::{Response, response},
        since_entry::{SinceEntry, since_entry},
        stress,
        training_done::Carried,
    },
    device::Measurand,
    gym::exercise::RepsExercise,
    plan::Span,
};
use jiff::civil::Date;

use crate::{
    devices::DeviceRegister,
    error::StoreError,
    one_rep_max::Tests,
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
    pub performance: Vec<Performance>,
    pub responses: Vec<Responding>,
}

/// What a test measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tested {
    /// In watts.
    Ftp,
    /// In grams.
    OneRepMax(RepsExercise),
}

/// One test's reading over the macrocycle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Performance {
    pub tested: Tested,
    /// `None` where the macrocycle holds no test of it yet.
    pub since: Option<SinceEntry>,
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
    /// `ftp` is every FTP test and `strength` every lift's 1RM tests, as their
    /// own use cases read them.
    ///
    /// **The macrocycle is the plan in force**, as the pages' ranges read it
    /// (#443), and its start is the plan's first day whatever was performed
    /// in its first weeks (operator, 2026-11-11).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn view(
        &self,
        today: Date,
        ftp: &[ftp_test::Test],
        strength: &[Tests],
    ) -> Result<View, StoreError> {
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
                performance: performance(ftp, strength, began, today),
                responses: vec![
                    read(Answering::RestingHeartRate, resting),
                    read(Answering::OvernightHrv, hrv),
                    read(Answering::Stress, stress),
                ],
            }),
        })
    }
}

/// Each test's reading over the macrocycle that `began` on a day: the FTP
/// test, then each lift's 1RM test.
fn performance(
    ftp: &[ftp_test::Test],
    strength: &[Tests],
    began: Date,
    today: Date,
) -> Vec<Performance> {
    let watts: Vec<(Date, u32)> = ftp
        .iter()
        .map(|test| (test.on(), test.ftp.watts().as_u32()))
        .collect();
    let lifts = strength.iter().map(|lift| Performance {
        tested: Tested::OneRepMax(lift.lift),
        since: since_entry(&lift.tests, began, today, one_rep_max::noise),
    });
    std::iter::once(Performance {
        tested: Tested::Ftp,
        since: since_entry(&watts, began, today, ftp_test::noise),
    })
    .chain(lifts)
    .collect()
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
