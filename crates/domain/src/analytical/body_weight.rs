//! Body weight as one time series (#250).
//!
//! **Every canonical weigh-in, once, in date order.** Body mass is
//! source-independent (§ 6), so the scale's readings and the ones typed into
//! a spreadsheet or MyFitnessPal are one series, and the canonical layer has
//! already made a weighing recorded twice one weigh-in (#409).
//!
//! **Ordered here, by the operator's day and then by the clock where one was
//! read**, so the series does not depend on the order a store returned rows in
//! (§ 9).
//!
//! **Nothing is filled in** (§ 37). A day with no weigh-in has no point, so a
//! gap in the record is a gap in the series.
//!
//! **The trend is the operator's, 2026-10-14** (#252): the mean of the
//! weigh-ins in the 7 days ending on a day, given at least 4 of those days have
//! one. The weigh-ins are discrete and the trend estimates a continuous mass,
//! so it is a line where they are points; a week spans one weekend whatever day
//! it ends on, which keeps the weekend's rise out of it. Where fewer than 4 days
//! were weighed there is no trend, and the line breaks rather than guessing
//! across (§ 37). It runs from the first weigh-in to the last, and never past.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::Weighed;
use crate::measure::Kg;

/// What the trend is called, so a different method is never mistaken for it.
pub const METHOD: &str = "mean of the 7 days ending on the day, at least 4 of them weighed";

/// How many days the trend is the mean of, the day itself included.
const WINDOW_DAYS: i64 = 7;

/// How many of them must have a weigh-in for there to be a trend.
const WEIGHED_DAYS: usize = 4;

/// The most the trend may move in a week and still be maintaining: half the
/// operator's aim of losing 0.25 kg a week (2026-10-14, #252).
const MAINTAINING_GRAMS: i64 = 125;

/// Which way the trend went over the week ending on a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Rising,
    Maintaining,
    Falling,
}

/// The trend on a day against the trend 7 days earlier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeekChange {
    pub on: Date,
    /// Positive when the trend rose.
    pub grams: i64,
    pub direction: Direction,
}

/// Body weight's trend on one day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trend {
    pub on: Date,
    pub mass: Kg,
}

/// Every weigh-in, the trend through them, and which way it went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Series {
    /// Oldest first, as [`body_weight`] orders them.
    pub weigh_ins: Vec<Weighed>,
    /// As [`trend`] gives it.
    pub trend: Vec<Vec<Trend>>,
    /// As [`week_changes`] gives them.
    pub changes: Vec<WeekChange>,
}

impl Series {
    pub fn of(weigh_ins: Vec<Weighed>) -> Self {
        let weigh_ins = body_weight(weigh_ins);
        let trend = trend(&weigh_ins);
        let changes = week_changes(&trend);
        Self {
            weigh_ins,
            trend,
            changes,
        }
    }
}

/// The body-weight series the canonical weigh-ins give, oldest first.
pub fn body_weight(mut weigh_ins: Vec<Weighed>) -> Vec<Weighed> {
    weigh_ins.sort_by_key(|weighed| {
        (
            weighed.at.day(),
            weighed.at.instant().map(|at| at.instant().as_second()),
        )
    });
    weigh_ins
}

/// The trend, oldest first, as unbroken runs of consecutive days.
///
/// A gap between two runs is where fewer than 4 of the 7 days were weighed.
pub fn trend(series: &[Weighed]) -> Vec<Vec<Trend>> {
    let mut by_day: BTreeMap<Date, Vec<u64>> = BTreeMap::new();
    for weighed in series {
        by_day
            .entry(weighed.at.day())
            .or_default()
            .push(weighed.mass.as_grams());
    }
    let (Some(&first), Some(&last)) = (by_day.keys().next(), by_day.keys().next_back()) else {
        return Vec::new();
    };

    let mut runs: Vec<Vec<Trend>> = Vec::new();
    let mut run: Vec<Trend> = Vec::new();
    for on in first
        .series(jiff::Span::new().days(1))
        .take_while(|on| *on <= last)
    {
        match mean_of_week(&by_day, on) {
            Some(mass) => run.push(Trend { on, mass }),
            None if run.is_empty() => {}
            None => runs.push(std::mem::take(&mut run)),
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

/// The mean of the weigh-ins in the week ending on `on`, if enough days of it
/// were weighed.
fn mean_of_week(by_day: &BTreeMap<Date, Vec<u64>>, on: Date) -> Option<Kg> {
    let start = on
        .checked_sub(jiff::Span::new().days(WINDOW_DAYS - 1))
        .ok()?;
    let week: Vec<&Vec<u64>> = by_day.range(start..=on).map(|(_, grams)| grams).collect();
    if week.len() < WEIGHED_DAYS {
        return None;
    }
    let grams: Vec<u64> = week.into_iter().flatten().copied().collect();
    let count = u64::try_from(grams.len()).ok()?;
    let total: u64 = grams.iter().sum();
    Some(Kg::from_grams((total + count / 2) / count))
}

/// Each day's change in the trend over the week, oldest first, wherever the
/// day and the day 7 before it both have a trend.
///
/// **Within 0.125 kg either way is maintaining.**
pub fn week_changes(trend: &[Vec<Trend>]) -> Vec<WeekChange> {
    let by_day: BTreeMap<Date, Kg> = trend
        .iter()
        .flatten()
        .map(|day| (day.on, day.mass))
        .collect();
    by_day
        .iter()
        .filter_map(|(&on, &mass)| {
            let before = on.checked_sub(jiff::Span::new().days(WINDOW_DAYS)).ok()?;
            let earlier = by_day.get(&before)?;
            let grams =
                i64::try_from(mass.as_grams()).ok()? - i64::try_from(earlier.as_grams()).ok()?;
            let direction = if grams > MAINTAINING_GRAMS {
                Direction::Rising
            } else if grams < -MAINTAINING_GRAMS {
                Direction::Falling
            } else {
                Direction::Maintaining
            };
            Some(WeekChange {
                on,
                grams,
                direction,
            })
        })
        .collect()
}
