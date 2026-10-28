//! Resting heart rate as a slow line for fitness, and a flag for illness (#417).
//!
//! **Two timescales, two readings** (operator, 2026-10-16). Training lowers
//! resting heart rate by a few beats over weeks to months (Reimers et al. 2018:
//! 2.7 to 5.8 bpm, in trials of about 12 weeks), and illness raises it above
//! the individual's own recent normal within days (Mishra et al. 2020). On the
//! operator's record a night bounces about 4 bpm around the week before it, as
//! large as the whole training effect, so the two are read apart.
//!
//! **The slow line is the mean of the 90 days ending on a day**, chosen from
//! his record against his training hours: shorter lines carry illness-sized
//! peaks that are the flag's to show, and this one is about six weeks late.
//!
//! **The flag is two nights running, each more than twice the standard
//! deviation above the mean of the 28 nights before it.** On the Forerunner 165
//! it caught both illnesses he knows of, Christmas 2025 and the one logged from
//! 2026-09-17, a night late, and fired twice more in 21 months; one night alone
//! fired 25 times.
//!
//! **Each window needs 3 in 7 of its days recorded**, as HRV's do: 39 of the
//! 90, 12 of the 28. Fewer, and the line breaks or the night is not read.
//!
//! **One device's days**, so a watch's figures are never read against
//! another's (§ 6).

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::{
    per_device::{DeviceSeries, Reading},
    range::{Range, runs_within},
};
use crate::measure::BeatsPerMinute;

/// What the slow line is.
pub const METHOD: &str = "mean of the 90 days ending on the day, at least 39 of them recorded";

/// What a flag is.
pub const FLAG: &str = "two nights running, each more than twice the standard deviation above the mean of the 28 nights before it";

const LINE_DAYS: i64 = 90;
const LINE_RECORDED: usize = 39;

const NORMAL_DAYS: i64 = 28;
const NORMAL_RECORDED: usize = 12;
const DEVIATIONS: f64 = 2.0;

/// The slow line on one day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Day {
    pub on: Date,
    pub bpm: f64,
}

/// A night high enough, after one that was too, to suggest illness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flag {
    pub on: Date,
    pub rate: BeatsPerMinute,
    /// The mean of the 28 nights before it.
    pub normal: f64,
}

/// One device's days, the slow line through them, and the flagged nights.
#[derive(Debug, Clone, PartialEq)]
pub struct Trended {
    pub series: DeviceSeries<BeatsPerMinute>,
    /// Oldest first, as unbroken runs of consecutive days.
    pub line: Vec<Vec<Day>>,
    /// Oldest first.
    pub flags: Vec<Flag>,
}

impl Trended {
    pub fn of(series: DeviceSeries<BeatsPerMinute>) -> Self {
        let line = line(&series.readings);
        let flags = flags(&series.readings);
        Self {
            series,
            line,
            flags,
        }
    }

    /// Only `range`'s days. The line and the flags on each are the whole
    /// record's.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        Self {
            series: self.series.within(range),
            line: runs_within(&self.line, range, |day| day.on),
            flags: self
                .flags
                .iter()
                .filter(|flag| range.covers(flag.on))
                .copied()
                .collect(),
        }
    }
}

/// The slow line, from the first day to the last and never past.
pub fn line(readings: &[Reading<BeatsPerMinute>]) -> Vec<Vec<Day>> {
    slow_line(&by_day(readings))
        .into_iter()
        .map(|run| run.into_iter().map(|(on, bpm)| Day { on, bpm }).collect())
        .collect()
}

/// The mean of the 90 days ending on each day, at least 39 of them recorded,
/// from the first day to the last and never past, as unbroken runs. Pulse
/// wave velocity's line is this one too (#471).
pub(super) fn slow_line(days: &BTreeMap<Date, f64>) -> Vec<Vec<(Date, f64)>> {
    let (Some(&first), Some(&last)) = (days.keys().next(), days.keys().next_back()) else {
        return Vec::new();
    };
    let mut runs: Vec<Vec<(Date, f64)>> = Vec::new();
    let mut run: Vec<(Date, f64)> = Vec::new();
    for on in first
        .series(jiff::Span::new().days(1))
        .take_while(|on| *on <= last)
    {
        let mean = window(days, on, 0, LINE_DAYS)
            .filter(|values| values.len() >= LINE_RECORDED)
            .map(|values| mean(&values));
        match mean {
            Some(value) => run.push((on, value)),
            None if run.is_empty() => {}
            None => runs.push(std::mem::take(&mut run)),
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

/// Every night that is high, and follows a night that was high, against the
/// 28 nights before each.
pub fn flags(readings: &[Reading<BeatsPerMinute>]) -> Vec<Flag> {
    let days = by_day(readings);
    let high = |on: Date| -> Option<f64> {
        let rate = *days.get(&on)?;
        let normal =
            window(&days, on, 1, NORMAL_DAYS).filter(|values| values.len() >= NORMAL_RECORDED)?;
        let mean = mean(&normal);
        let deviation = deviation(&normal, mean)?;
        (rate - mean > DEVIATIONS * deviation).then_some(mean)
    };
    readings
        .iter()
        .filter_map(|reading| {
            let normal = high(reading.on)?;
            let before = reading.on.yesterday().ok()?;
            high(before)?;
            Some(Flag {
                on: reading.on,
                rate: reading.value,
                normal,
            })
        })
        .collect()
}

fn by_day(readings: &[Reading<BeatsPerMinute>]) -> BTreeMap<Date, f64> {
    readings
        .iter()
        .map(|reading| (reading.on, f64::from(reading.value.as_u32())))
        .collect()
}

/// The days in the `length` days ending `before` days ahead of `on`.
fn window(days: &BTreeMap<Date, f64>, on: Date, before: i64, length: i64) -> Option<Vec<f64>> {
    let to = on.checked_sub(jiff::Span::new().days(before)).ok()?;
    let from = to.checked_sub(jiff::Span::new().days(length - 1)).ok()?;
    Some(days.range(from..=to).map(|(_, value)| *value).collect())
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / f64::from(u32::try_from(values.len()).unwrap_or(u32::MAX))
}

/// The sample standard deviation; `None` for fewer than two.
fn deviation(values: &[f64], mean: f64) -> Option<f64> {
    let count = u32::try_from(values.len())
        .ok()
        .filter(|count| *count > 1)?;
    let squares: f64 = values.iter().map(|value| (value - mean).powi(2)).sum();
    Some((squares / f64::from(count - 1)).sqrt())
}
