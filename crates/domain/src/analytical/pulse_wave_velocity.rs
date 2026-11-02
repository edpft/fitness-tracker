//! Pulse wave velocity, as a slow line (#471).
//!
//! **A general health measure, read over a prolonged period so that what
//! confounds it averages out** (operator, 2026-10-28). It follows blood
//! pressure at the moment of measurement, about 1 m/s per 10 mmHg diastolic
//! (Spronck et al. 2017), and on his record it swings 0.5 to 0.8 m/s over a few
//! weeks with the standing heart rate the scale takes. The line is resting
//! heart rate's, the mean of the 90 days ending on a day, at least 39 of them
//! recorded ([`super::resting`]), and its band counts those swings.
//!
//! **Read as resting heart rate's is** (operator, 2026-11-02, #688): the line
//! against itself 90 days before, falling or rising beyond the noise of the
//! two and no clear change otherwise. Lower is better: each 1 m/s of aortic
//! pulse wave velocity carries about 14% more cardiovascular events
//! (Vlachopoulos et al. 2010). Aerobic training lowers it 0.4 to 0.6 m/s
//! (Ashor et al. 2014), where two lines on his record differ by about ± 0.22
//! m/s by chance, so a training-sized fall is beyond the noise. Whether the
//! scale tracks one person's change is not validated (Collier et al. 2020
//! compared it across 20 students only).
//!
//! **Vascular age is not drawn.** On his record it is 24.0 + 2.59 × pulse wave
//! velocity + 0.98 × years elapsed, give or take 0.1 years: the same figure and
//! the calendar. **Nor are published bands for age**, given the scale's
//! possible systematic error against the clinical method (Collier et al. 2020).
//!
//! **The line's band is [`super::confidence`]'s**, and it matters most here.
//! A reading barely moves from the day before's (σ 0.07 m/s between
//! consecutive days) but drifts over a fortnight with what confounds it
//! (σ 0.21 about the 90-day mean, correlation 0.92 a day apart, gone by two
//! weeks). So 80 readings in 90 days hold about 7 independent ones and the
//! band is about ± 0.15 m/s, where σ/√n from consecutive days said ± 0.02.
//! Readings bunched into a fortnight count as about one.
//!
//! **A method is the scale's algorithm** (§ 6): a reading by another is a
//! series of its own, and no line joins two.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::{
    per_device::Reading,
    range::Range,
    range::runs_within,
    resting::{Change, LINE_DAYS, slow_line},
};
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
    /// How far either side its 95% band runs, in metres per second; `None`
    /// where the record is too short to measure the noise.
    pub margin: Option<f64>,
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
    /// The line's latest day against the line 90 days before it; `None`
    /// where either has no line.
    #[must_use]
    pub fn change(&self) -> Option<Change> {
        let mut days = self.line.iter().flatten();
        let now = days.clone().next_back()?;
        let before = now.on.checked_sub(jiff::Span::new().days(LINE_DAYS)).ok()?;
        let then = days.find(|day| day.on == before)?;
        Some(Change::of(
            now.on,
            (now.metres_per_second, now.margin),
            (then.metres_per_second, then.margin),
        ))
    }

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
                        .map(|(on, metres_per_second, margin)| Day {
                            on,
                            metres_per_second,
                            margin,
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
    use crate::{
        analytical::{body_weight::Direction, range::Range},
        body::PulseWaveVelocity,
    };

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
    fn the_line_carries_its_band_and_fewer_readings_widen_it() {
        let mut readings = daily(200, 0);
        let full = by_algorithm(&readings);
        let on = date(2026, 6, 1);
        let margin = |series: &[super::Series]| {
            series[0]
                .line
                .iter()
                .flatten()
                .find(|day| day.on == on)
                .and_then(|day| day.margin)
                .expect("a band on 1 June")
        };
        let wide = margin(&full);
        // σ is 0.2 about a level of 7.2, and no day follows the one before.
        assert!(
            (wide - 1.96 * 0.2 / 90.0_f64.sqrt()).abs() < 0.005,
            "{wide}"
        );
        readings.retain(|one| one.on.day() % 2 == 0 || one.on.month() != 5);
        assert!(margin(&by_algorithm(&readings)) > wide);
    }

    #[test]
    fn the_line_reads_falling_beyond_its_noise_and_no_clear_change_within_it() {
        let steady = by_algorithm(&daily(200, 0));
        let change = steady[0].change().expect("a line 90 days before");
        assert_eq!(change.on, date(2026, 7, 19));
        assert!(change.by.abs() < 0.01, "{change:?}");
        assert_eq!(change.direction, None);

        // A metre a second lower from 21 April: the line on 19 July is all
        // of it, the line 90 days before none.
        let fell: Vec<Measured> = daily(200, 0)
            .into_iter()
            .map(|one| {
                if one.on >= date(2026, 4, 21) {
                    measured(one.on, 0, one.velocity.as_millimetres_per_second() - 1_000)
                } else {
                    one
                }
            })
            .collect();
        let change = by_algorithm(&fell)[0].change().expect("a line");
        assert!((change.by + 1.0).abs() < 0.01, "{change:?}");
        assert_eq!(change.direction, Some(Direction::Falling));

        assert_eq!(by_algorithm(&daily(100, 0))[0].change(), None);
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
