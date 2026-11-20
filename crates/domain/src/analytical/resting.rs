//! A slow line, first drawn through resting heart rate (#417), and now
//! pulse wave velocity's (#471).
//!
//! **The slow line is the mean of the 90 days ending on a day**, chosen from
//! his resting heart rate against his training hours: training lowers it by a
//! few beats over weeks to months (Reimers et al. 2018: 2.7 to 5.8 bpm, in
//! trials of about 12 weeks), and shorter lines carry illness-sized peaks.
//!
//! **The window needs 3 in 7 of its days recorded**: 39 of the 90. Fewer, and
//! the line breaks.
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

use super::{body_weight::Direction, confidence::Noise};

pub(super) const LINE_DAYS: i64 = 90;
const LINE_RECORDED: usize = 39;

/// The slow line on one day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Day {
    pub on: Date,
    pub bpm: f64,
    /// How far either side its 95% band runs; `None` where the record is too
    /// short to measure the noise.
    pub margin: Option<f64>,
}

/// A line on one day against the line [`LINE_DAYS`] days before: pulse
/// wave velocity's (#688).
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

/// The days in the `length` days ending `before` days ahead of `on`.
fn window(days: &BTreeMap<Date, f64>, on: Date, before: i64, length: i64) -> Option<Vec<f64>> {
    let to = on.checked_sub(jiff::Span::new().days(before)).ok()?;
    let from = to.checked_sub(jiff::Span::new().days(length - 1)).ok()?;
    Some(days.range(from..=to).map(|(_, value)| *value).collect())
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / f64::from(u32::try_from(values.len()).unwrap_or(u32::MAX))
}
