//! Pulse wave velocity, as a slow line (#471).
//!
//! **A general health measure, read over a prolonged period so that what
//! confounds it averages out** (operator, 2026-10-28). It follows blood
//! pressure at the moment of measurement, about 1 m/s per 10 mmHg diastolic
//! (Spronck et al. 2017), and on his record it swings 0.5 to 0.8 m/s over a few
//! weeks with the standing heart rate the scale takes, where a training block
//! moves it about 0.4 m/s (Ashor et al. 2014). So there is no direction: the
//! line is resting heart rate's, the mean of the 90 days ending on a day, at
//! least 39 of them recorded ([`super::resting`]).
//!
//! **Vascular age is not drawn.** On his record it is 24.0 + 2.59 × pulse wave
//! velocity + 0.98 × years elapsed, give or take 0.1 years: the same figure and
//! the calendar. **Nor are published bands for age**, given the scale's
//! possible systematic error against the clinical method (Collier et al. 2020).
//!
//! **A method is the scale's algorithm** (§ 6): a reading by another is a
//! series of its own, and no line joins two.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::{per_device::Reading, range::Range, range::runs_within, resting::slow_line};
use crate::body::PulseWaveVelocity;

/// What the line is.
pub const METHOD: &str = "mean of the 90 days ending on the day, at least 39 of them recorded";

/// One reading, and the algorithm that took it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measured {
    pub on: Date,
    pub algorithm: u64,
    pub velocity: PulseWaveVelocity,
}

/// The line on one day, in metres per second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Day {
    pub on: Date,
    pub metres_per_second: f64,
}

/// Every reading one algorithm took, oldest first, and the line through them.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub algorithm: u64,
    pub readings: Vec<Reading<PulseWaveVelocity>>,
    /// Unbroken runs of consecutive days, oldest first.
    pub line: Vec<Vec<Day>>,
}

impl Series {
    /// Only `range`'s days. The line on each is the whole record's.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        Self {
            algorithm: self.algorithm,
            readings: self
                .readings
                .iter()
                .filter(|reading| range.covers(reading.on))
                .copied()
                .collect(),
            line: runs_within(&self.line, range, |day| day.on),
        }
    }
}

/// One series per algorithm, in the order each first measured.
#[must_use]
pub fn by_algorithm(measured: &[Measured]) -> Vec<Series> {
    let mut order: Vec<u64> = Vec::new();
    let mut readings: BTreeMap<u64, Vec<Reading<PulseWaveVelocity>>> = BTreeMap::new();
    let mut sorted = measured.to_vec();
    sorted.sort_by_key(|one| one.on);
    for one in sorted {
        if !order.contains(&one.algorithm) {
            order.push(one.algorithm);
        }
        readings.entry(one.algorithm).or_default().push(Reading {
            on: one.on,
            value: one.velocity,
        });
    }
    order
        .into_iter()
        .map(|algorithm| {
            let readings = readings.remove(&algorithm).unwrap_or_default();
            let line = slow_line(&by_day(&readings))
                .into_iter()
                .map(|run| {
                    run.into_iter()
                        .map(|(on, metres_per_second)| Day {
                            on,
                            metres_per_second,
                        })
                        .collect()
                })
                .collect();
            Series {
                algorithm,
                readings,
                line,
            }
        })
        .collect()
}

/// Each day's mean, in metres per second, where a day was measured twice.
fn by_day(readings: &[Reading<PulseWaveVelocity>]) -> BTreeMap<Date, f64> {
    let mut days: BTreeMap<Date, Vec<f64>> = BTreeMap::new();
    for reading in readings {
        days.entry(reading.on)
            .or_default()
            .push(f64::from(reading.value.as_millimetres_per_second()) / 1_000.0);
    }
    days.into_iter()
        .map(|(on, values)| {
            let count = f64::from(u32::try_from(values.len()).unwrap_or(u32::MAX));
            (on, values.iter().sum::<f64>() / count)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Measured, by_algorithm};
    use crate::{analytical::range::Range, body::PulseWaveVelocity};

    fn measured(on: Date, algorithm: u64, millimetres: u32) -> Measured {
        Measured {
            on,
            algorithm,
            velocity: PulseWaveVelocity::from_millimetres_per_second(millimetres),
        }
    }

    /// One reading a day for `days` days from 1 January, alternating 7.0 and
    /// 7.4 m/s.
    fn daily(days: i64, algorithm: u64) -> Vec<Measured> {
        date(2026, 1, 1)
            .series(jiff::Span::new().days(1))
            .take(usize::try_from(days).unwrap_or(0))
            .enumerate()
            .map(|(index, on)| measured(on, algorithm, if index % 2 == 0 { 7_000 } else { 7_400 }))
            .collect()
    }

    #[test]
    fn the_line_starts_once_39_days_are_measured() {
        let series = by_algorithm(&daily(60, 0));
        assert_eq!(series.len(), 1);
        let line: Vec<_> = series[0].line.iter().flatten().collect();
        assert_eq!(line.first().map(|day| day.on), Some(date(2026, 2, 8)));
        assert_eq!(line.last().map(|day| day.on), Some(date(2026, 3, 1)));
        assert!(
            line.iter()
                .all(|day| (day.metres_per_second - 7.2).abs() < 0.02),
            "the mean of alternating 7.0 and 7.4"
        );
    }

    #[test]
    fn a_long_gap_breaks_the_line_and_a_short_one_does_not() {
        let mut readings = daily(60, 0);
        readings.retain(|one| !(date(2026, 2, 20)..=date(2026, 2, 24)).contains(&one.on));
        let series = by_algorithm(&readings);
        assert_eq!(series[0].line.len(), 1, "five days missing out of 90");
        assert_eq!(series[0].readings.len(), 55, "the gap is not filled");

        let mut sparse = daily(250, 0);
        sparse.retain(|one| !(date(2026, 3, 1)..=date(2026, 5, 31)).contains(&one.on));
        let series = by_algorithm(&sparse);
        assert_eq!(
            series[0].line.len(),
            2,
            "three months missing: {:?}",
            series[0].line
        );
    }

    #[test]
    fn each_algorithm_is_its_own_series() {
        let mut readings = daily(50, 0);
        readings.extend(daily(50, 1).into_iter().map(|one| {
            measured(
                one.on
                    .checked_add(jiff::Span::new().days(50))
                    .expect("a date"),
                1,
                8_000,
            )
        }));
        let series = by_algorithm(&readings);
        assert_eq!(
            series.iter().map(|one| one.algorithm).collect::<Vec<_>>(),
            [0, 1]
        );
        assert!(
            series[1]
                .line
                .iter()
                .flatten()
                .all(|day| (day.metres_per_second - 8.0).abs() < 1e-9),
            "nothing of the first algorithm in the second's line"
        );
    }

    #[test]
    fn a_range_keeps_the_whole_record_s_line() {
        let series = by_algorithm(&daily(60, 0));
        let range = Range {
            from: date(2026, 2, 20),
            to: date(2026, 3, 1),
        };
        let within = series[0].within(range);
        assert_eq!(within.readings.len(), 10);
        let line: Vec<_> = within.line.iter().flatten().collect();
        assert_eq!(line.first().map(|day| day.on), Some(date(2026, 2, 20)));
        assert!((line[0].metres_per_second - 7.2).abs() < 0.02);
    }
}
