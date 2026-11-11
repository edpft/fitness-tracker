//! A day's stress as the response reads it (#517, #777).
//!
//! **Only a day measured over 20 hours or more.** The figure is the whole
//! day's average, asleep and awake, and a day the watch was worn for part of
//! reads whichever part that was: on the operator's record 329 of the 2,679
//! days with an average were measured for less. The 20 is the mocks', which
//! the operator read and signed off on 2026-11-11, and has not been ruled on
//! by itself.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::per_device::Reading;
use crate::{body::StressLevel, measure::PositiveDuration};

/// How long a day has to have been measured over to be a whole day's average.
const WHOLE_DAY_SECONDS: u64 = 20 * 60 * 60;

/// One day's average, and how much of the day it was measured over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Measured {
    pub average: StressLevel,
    pub over: PositiveDuration,
}

/// Each whole day's average.
pub fn whole_days(readings: &[Reading<Measured>]) -> BTreeMap<Date, f64> {
    readings
        .iter()
        .filter(|reading| reading.value.over.as_seconds() >= WHOLE_DAY_SECONDS)
        .map(|reading| (reading.on, f64::from(reading.value.average.as_u8())))
        .collect()
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::{Measured, whole_days};
    use crate::{analytical::per_device::Reading, body::StressLevel, measure::PositiveDuration};

    #[test]
    fn a_day_measured_for_a_morning_is_not_a_days_average() {
        let day = |on, level, seconds| Reading {
            on,
            value: Measured {
                average: StressLevel::new(level).expect("a level"),
                over: PositiveDuration::from_seconds(seconds).expect("a duration"),
            },
        };
        let days = whole_days(&[
            day(date(2026, 10, 19), 41, 6 * 3600),
            day(date(2026, 10, 20), 28, 24 * 3600),
            day(date(2026, 10, 21), 30, 20 * 3600),
        ]);
        assert_eq!(
            days.into_iter().collect::<Vec<_>>(),
            vec![(date(2026, 10, 20), 28.0), (date(2026, 10, 21), 30.0)]
        );
    }
}
