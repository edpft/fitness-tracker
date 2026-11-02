//! Overnight HRV as a trend against its own band (#417).
//!
//! **The method is Plews et al.'s** (operator, 2026-10-16): the mean of log
//! rMSSD over the 7 nights ending on a day, against the 60 nights before
//! those 7. Garmin's nightly figure is RMSSD averaged over sleep, so it is
//! log-transformed as it stands.
//!
//! **The band is the noise, not a fraction of the CV** (operator, 2026-11-02,
//! #688): the baseline's mean ± 1.96 × its nights' SD × √(1/week + 1/baseline),
//! in log units and only shown in milliseconds, so a week outside it differs
//! from the baseline beyond chance. Nights do not carry over (#616), so σ/√n
//! holds. ± 1 × CV, a fraction picked from Plews's papers, is withdrawn:
//! Buchheit (2014) finds no evidence that any fraction is meaningful, and the
//! literature gives a direction but no size for one person's change (Bellenger
//! et al. 2016). On the Forerunner 165 a full week's band is about ± 15%, and
//! 117 of 631 days read below and 60 above.
//!
//! **Each night is its device's**, so this runs over one
//! [`DeviceSeries`](super::per_device::DeviceSeries) and a band never mixes two
//! watches (§ 6).
//!
//! **A gap is a gap** (§ 37). With fewer than 3 of the 7 nights recorded there
//! is no trend that day (Plews, Laursen & Buchheit 2016 drop such a week) and
//! the line breaks; with fewer than 26 of the 60, the same proportion, there is
//! no band and nothing to read the trend against.
//!
//! **The 7 nights' variation is not read** (operator, 2026-11-02, #688): its
//! only published use is one over-reached triathlete (Plews et al. 2012), with
//! no size and no normal range to read a figure against.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::{
    per_device::{DeviceSeries, Reading},
    range::{Range, runs_within},
};
use crate::measure::HeartRateVariability;

/// What the trend is, so a different method is never mistaken for it.
pub const METHOD: &str =
    "mean of log rMSSD over the 7 nights ending on the day, at least 3 of them recorded";

/// What the band is.
pub const BAND: &str = "the mean of the 60 nights before those 7, at least 26 of them recorded, ± how far a week can differ from it by chance, at 95%";

/// The normal deviate for a two-sided 95% interval.
const Z: f64 = 1.96;

/// The nights a trend is the mean of, the day itself included.
const WEEK_NIGHTS: i64 = 7;
const WEEK_RECORDED: usize = 3;

/// The nights before the week that the band is taken from.
const BASELINE_NIGHTS: i64 = 60;
const BASELINE_RECORDED: usize = 26;

/// Where a day's trend sits against its band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Against {
    Below,
    Within,
    Above,
}

/// The baseline, and how far from it a week can be by chance, in
/// milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub low: f64,
    /// The baseline nights' mean of log rMSSD, back in milliseconds.
    pub normal: f64,
    pub high: f64,
}

/// The trend on one day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Day {
    pub on: Date,
    /// The 7 nights' mean of log rMSSD, back in milliseconds.
    pub milliseconds: f64,
    /// `None` with fewer than 26 of the 60 nights before.
    pub band: Option<Band>,
}

impl Day {
    /// `None` where there is no band.
    pub fn against(&self) -> Option<Against> {
        let band = self.band?;
        Some(if self.milliseconds < band.low {
            Against::Below
        } else if self.milliseconds > band.high {
            Against::Above
        } else {
            Against::Within
        })
    }
}

/// The trend, oldest first, as unbroken runs of consecutive days, from the
/// first night to the last and never past.
pub fn trend(readings: &[Reading<HeartRateVariability>]) -> Vec<Vec<Day>> {
    let nights: BTreeMap<Date, f64> = readings
        .iter()
        .map(|reading| (reading.on, f64::from(reading.value.as_milliseconds()).ln()))
        .collect();
    let (Some(&first), Some(&last)) = (nights.keys().next(), nights.keys().next_back()) else {
        return Vec::new();
    };

    let mut runs: Vec<Vec<Day>> = Vec::new();
    let mut run: Vec<Day> = Vec::new();
    for on in first
        .series(jiff::Span::new().days(1))
        .take_while(|on| *on <= last)
    {
        match day(&nights, on) {
            Some(day) => run.push(day),
            None if run.is_empty() => {}
            None => runs.push(std::mem::take(&mut run)),
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

fn day(nights: &BTreeMap<Date, f64>, on: Date) -> Option<Day> {
    let week = window(nights, on, 0, WEEK_NIGHTS)?;
    if week.len() < WEEK_RECORDED {
        return None;
    }
    let mean = week.iter().sum::<f64>() / count(&week);
    let band = window(nights, on, WEEK_NIGHTS, BASELINE_NIGHTS)
        .filter(|baseline| baseline.len() >= BASELINE_RECORDED)
        .and_then(|baseline| {
            let (normal, spread) = mean_and_deviation(&baseline)?;
            let noise = Z * spread * (count(&week).recip() + count(&baseline).recip()).sqrt();
            Some(Band {
                low: (normal - noise).exp(),
                normal: normal.exp(),
                high: (normal + noise).exp(),
            })
        });
    Some(Day {
        on,
        milliseconds: mean.exp(),
        band,
    })
}

/// How many values, as a float.
fn count(values: &[f64]) -> f64 {
    f64::from(u32::try_from(values.len()).unwrap_or(u32::MAX))
}

/// The nights in the `length` days ending `before` days ahead of `on`.
fn window(nights: &BTreeMap<Date, f64>, on: Date, before: i64, length: i64) -> Option<Vec<f64>> {
    let to = on.checked_sub(jiff::Span::new().days(before)).ok()?;
    let from = to.checked_sub(jiff::Span::new().days(length - 1)).ok()?;
    Some(nights.range(from..=to).map(|(_, value)| *value).collect())
}

/// The mean and the sample standard deviation.
fn mean_and_deviation(values: &[f64]) -> Option<(f64, f64)> {
    let count = u32::try_from(values.len())
        .ok()
        .filter(|count| *count > 1)?;
    let mean = values.iter().sum::<f64>() / f64::from(count);
    let squares: f64 = values.iter().map(|value| (value - mean).powi(2)).sum();
    Some((mean, (squares / f64::from(count - 1)).sqrt()))
}

/// One device's nights and the trend through them.
#[derive(Debug, Clone, PartialEq)]
pub struct Trended {
    pub series: DeviceSeries<HeartRateVariability>,
    /// As [`trend`] gives it.
    pub trend: Vec<Vec<Day>>,
}

impl Trended {
    pub fn of(series: DeviceSeries<HeartRateVariability>) -> Self {
        let trend = trend(&series.readings);
        Self { series, trend }
    }

    /// Only `range`'s days. The trend on each is the whole record's.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        Self {
            series: self.series.within(range),
            trend: runs_within(&self.trend, range, |day| day.on),
        }
    }
}
