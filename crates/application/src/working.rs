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
//! **The readings are `domain`'s** ([`since_entry`], [`response`]); this
//! hands each the series it reads. **Each response reading is read alone**
//! (#830), from its own series and the training done, so a new night of HRV
//! reads HRV again and nothing else. Nothing is written: each re-derives from
//! what it reads exactly (§ 5).
//!
//! **One watch's days**, the one that recorded the latest of each reading: a
//! watch's figures are never read against another's (§ 6), and the line the
//! expected result is read off is drawn through that watch's weeks alone.

use std::collections::BTreeMap;

use domain::{
    analytical::{
        composition::{ComponentSeries, Unit},
        ftp_test, heart_rate_at_power, hrv, one_rep_max,
        per_device::{DeviceSeries, SentBy},
        response::{Response, response},
        resting,
        since_entry::{SinceEntry, since_entry},
        since_start::{SinceStart, since_start},
        stress,
        training_done::Carried,
    },
    body::Component,
    gym::exercise::RepsExercise,
    plan::{Plan, Span},
    planner::Discipline,
};
use jiff::civil::Date;

use crate::one_rep_max::{Lift, Tests};

/// The macrocycle in force on a day, as every card measured against it reads
/// it: when it began and the days of its mesocycles (#830).
///
/// **The macrocycle is the plan in force**, as the pages' ranges read it
/// (#443), and its start is the plan's first day whatever was performed in its
/// first weeks (operator, 2026-11-11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Its intended start: the first day of the plan.
    pub began: Date,
    /// The days of each of its mesocycles, oldest first.
    pub mesocycles: Vec<Span>,
    /// The first day of the macrocycle before it, where there was one.
    pub last_began: Option<Date>,
}

impl Frame {
    /// The macrocycle in force on `today` among `plans`, the plans as they
    /// now stand; `None` on a day no plan is in force.
    #[must_use]
    pub fn on(today: Date, plans: &[Plan]) -> Option<Self> {
        let plan = plans.iter().find(|plan| plan.span().covers(today))?;
        let began = plan.span().start();
        Some(Self {
            began,
            mesocycles: plan.mesocycle_spans(),
            last_began: plans
                .iter()
                .map(|plan| plan.span().start())
                .filter(|start| *start < began)
                .max(),
        })
    }
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

/// Resting heart rate over the macrocycle that `began`, through `today`,
/// against the training `carried`.
#[must_use]
pub fn resting(
    trended: &[resting::Trended],
    carried: &Carried,
    began: Date,
    today: Date,
) -> Responding {
    responding(
        Answering::RestingHeartRate,
        trended.iter().map(|one| &one.series),
        |reading| f64::from(reading.as_u32()),
        carried,
        began,
        today,
    )
}

/// Overnight HRV over the macrocycle that `began`, through `today`, against
/// the training `carried`.
#[must_use]
pub fn overnight_hrv(
    trended: &[hrv::Trended],
    carried: &Carried,
    began: Date,
    today: Date,
) -> Responding {
    responding(
        Answering::OvernightHrv,
        trended.iter().map(|one| &one.series),
        |reading| f64::from(reading.as_milliseconds()),
        carried,
        began,
        today,
    )
}

/// Stress over the macrocycle that `began`, through `today`, against the
/// training `carried`: each whole day's average.
#[must_use]
pub fn stress(
    series: &[DeviceSeries<stress::Measured>],
    carried: &Carried,
    began: Date,
    today: Date,
) -> Responding {
    let latest = latest(series, today);
    Responding {
        reading: Answering::Stress,
        sent_by: latest.map(|series| series.sent_by.clone()),
        response: latest.and_then(|series| {
            response(&stress::whole_days(&series.readings), carried, began, today)
        }),
    }
}

/// One response reading, on the watch that recorded its latest day, each
/// day's figure as `figure` reads it.
fn responding<'a, T: Copy + 'a>(
    reading: Answering,
    series: impl IntoIterator<Item = &'a DeviceSeries<T>>,
    figure: impl Fn(T) -> f64,
    carried: &Carried,
    began: Date,
    today: Date,
) -> Responding {
    let latest = latest(series, today);
    Responding {
        reading,
        sent_by: latest.map(|series| series.sent_by.clone()),
        response: latest.and_then(|series| {
            let days: BTreeMap<Date, f64> = series
                .readings
                .iter()
                .map(|day| (day.on, figure(day.value)))
                .collect();
            response(&days, carried, began, today)
        }),
    }
}

/// The FTP test's reading over the macrocycle that `began`, with heart rate
/// at 140 W, one series per device, read beside it.
#[must_use]
pub fn cycling(
    ftp_tests: &[ftp_test::Test],
    at_power: &[heart_rate_at_power::Trended],
    began: Date,
    today: Date,
) -> Performance {
    Performance {
        tested: Tested::Ftp,
        since: ftp(ftp_tests, began, today),
        estimated: self::at_power(at_power, began, today),
    }
}

/// The latest FTP test over the macrocycle that `began`, against its entry
/// test; `None` where it holds no test.
#[must_use]
pub fn ftp(tests: &[ftp_test::Test], began: Date, today: Date) -> Option<SinceEntry> {
    let watts: Vec<(Date, u32)> = tests
        .iter()
        .map(|test| (test.on(), test.ftp.watts().as_u32()))
        .collect();
    since_entry(&watts, began, today, ftp_test::noise)
}

/// Each lift's 1RM test over the macrocycle that `began`, with its estimate
/// read beside it, and each lift estimated but never tested.
#[must_use]
pub fn gym(strength: &[Tests], estimated: &[Lift], began: Date, today: Date) -> Vec<Performance> {
    let estimate = |lift: RepsExercise| {
        let trend = &estimated.iter().find(|one| one.lift == lift)?.absolute;
        let change = trend.change?;
        let now = trend.now()?;
        Some(SinceStart {
            from: now - change.by,
            now,
            margin: Some(change.margin),
        })
    };
    let lifts = strength.iter().map(|lift| Performance {
        tested: Tested::OneRepMax(lift.lift),
        since: since_entry(&lift.tests, began, today, one_rep_max::noise),
        estimated: estimate(lift.lift),
    });
    let estimated_only = estimated
        .iter()
        .filter(|one| !strength.iter().any(|tests| tests.lift == one.lift))
        .map(|one| Performance {
            tested: Tested::OneRepMax(one.lift),
            since: None,
            estimated: estimate(one.lift),
        });
    lifts.chain(estimated_only).collect()
}

/// Heart rate at 140 W since the start, on the line of the device that rode
/// last: a device's line is never read against another's (§ 6).
#[must_use]
pub fn at_power(
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
/// latest.
#[must_use]
pub fn component(
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
fn latest<'a, T: 'a>(
    series: impl IntoIterator<Item = &'a DeviceSeries<T>>,
    today: Date,
) -> Option<&'a DeviceSeries<T>> {
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
