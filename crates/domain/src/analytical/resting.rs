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
//!
//! **The line's band is [`super::confidence`]'s**, and here a night follows
//! the nights before it: on the Forerunner 165 the correlation is 0.41 a
//! night apart and still 0.3 a week apart, fading over a fortnight, because an
//! illness, a hard block or a run of short nights raises several in a row. So
//! 90 nights hold about 13 independent ones, and the band, σ about 3.1 bpm, is
//! about ± 1.7 bpm where σ/√n would say ± 0.6.
//!
//! **The line is read against itself 90 days before** (#615): rising or
//! falling where the change is beyond the noise of the two lines, and no clear
//! change otherwise (operator, 2026-11-02). There is no steady: at this noise
//! a reading cannot show that nothing moved. On the Forerunner 165 two lines
//! differ by about ± 2.4 bpm by chance, and endurance training lowers resting
//! heart rate by 2.7 to 5.8 bpm over about 12 weeks (Reimers et al. 2018), so
//! the training effect sits at the noise or above it. On his record the
//! Forerunner 165 read falling on 39 days of 553 and the Vivoactive 3 moved on
//! 543 of 1,885 (#688).

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::{
    body_weight::Direction,
    confidence::Noise,
    per_device::{DeviceSeries, Reading},
    range::{Range, runs_within},
};
use crate::measure::BeatsPerMinute;

/// What the slow line is.
pub const METHOD: &str = "mean of the 90 days ending on the day, at least 39 of them recorded";

/// What a flag is.
pub const FLAG: &str = "two nights running, each more than twice the standard deviation above the mean of the 28 nights before it";

pub(super) const LINE_DAYS: i64 = 90;
const LINE_RECORDED: usize = 39;

const NORMAL_DAYS: i64 = 28;
const NORMAL_RECORDED: usize = 12;
const DEVIATIONS: f64 = 2.0;

/// The slow line on one day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Day {
    pub on: Date,
    pub bpm: f64,
    /// How far either side its 95% band runs; `None` where the record is too
    /// short to measure the noise.
    pub margin: Option<f64>,
}

/// A night high enough, after one that was too, to suggest illness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flag {
    pub on: Date,
    pub rate: BeatsPerMinute,
    /// The mean of the 28 nights before it.
    pub normal: f64,
}

/// The line on one day against the line [`LINE_DAYS`] days before. Pulse
/// wave velocity's line is read the same way (#688).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    pub on: Date,
    /// In the line's unit, negative where the line fell.
    pub by: f64,
    /// How far either side its 95% band runs: the two lines' margins
    /// combined. `None` where either has none.
    pub margin: Option<f64>,
    /// Rising or falling where the whole band is one side of no change;
    /// `None` otherwise, and wherever there is no band to read.
    pub direction: Option<Direction>,
}

impl Change {
    /// `now` against `then`, read by the rule.
    #[must_use]
    pub fn between(now: &Day, then: &Day) -> Self {
        Self::of(now.on, (now.bpm, now.margin), (then.bpm, then.margin))
    }

    /// The line on `on`, and its margin, against the line and its margin
    /// [`LINE_DAYS`] days before.
    #[must_use]
    pub fn of(on: Date, now: (f64, Option<f64>), then: (f64, Option<f64>)) -> Self {
        let by = now.0 - then.0;
        let margin = now.1.zip(then.1).map(|(now, then)| now.hypot(then));
        let direction = margin.and_then(|margin| {
            if by - margin > 0.0 {
                Some(Direction::Rising)
            } else if by + margin < 0.0 {
                Some(Direction::Falling)
            } else {
                None
            }
        });
        Self {
            on,
            by,
            margin,
            direction,
        }
    }
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

    /// The line's latest day against the line [`LINE_DAYS`] days before it;
    /// `None` where either has no line.
    #[must_use]
    pub fn change(&self) -> Option<Change> {
        let mut days = self.line.iter().flatten();
        let now = days.clone().next_back()?;
        let before = now.on.checked_sub(jiff::Span::new().days(LINE_DAYS)).ok()?;
        let then = days.find(|day| day.on == before)?;
        Some(Change::between(now, then))
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
        .map(|run| {
            run.into_iter()
                .map(|(on, bpm, margin)| Day { on, bpm, margin })
                .collect()
        })
        .collect()
}

/// The mean of the 90 days ending on each day, at least 39 of them recorded,
/// and its band's margin, from the first day to the last and never past, as
/// unbroken runs. Pulse wave velocity's line is this one too (#471).
pub(super) fn slow_line(days: &BTreeMap<Date, f64>) -> Vec<Vec<(Date, f64, Option<f64>)>> {
    let (Some(&first), Some(&last)) = (days.keys().next(), days.keys().next_back()) else {
        return Vec::new();
    };
    let noise = Noise::of(days, LINE_DAYS, LINE_RECORDED);
    let mut runs: Vec<Vec<(Date, f64, Option<f64>)>> = Vec::new();
    let mut run: Vec<(Date, f64, Option<f64>)> = Vec::new();
    for on in first
        .series(jiff::Span::new().days(1))
        .take_while(|on| *on <= last)
    {
        let mean = window(days, on, 0, LINE_DAYS)
            .filter(|values| values.len() >= LINE_RECORDED)
            .map(|values| mean(&values));
        match mean {
            Some(value) => run.push((
                on,
                value,
                noise.as_ref().and_then(|noise| noise.margin(days, on)),
            )),
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
