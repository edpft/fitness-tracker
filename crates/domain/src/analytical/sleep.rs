//! A night's sleep as the series read it, and the trend through each (#255,
//! #418).
//!
//! **Both figures are read against a published band** (operator,
//! 2026-10-28: *"we're comparing my figures to known guidelines"*), and each
//! has a direction within it, since a trend in the best band can still be
//! sliding out of it.
//!
//! - **Time asleep** against the NHS's 7 to 9 hours for adults.
//! - **Regularity** is the Sleep Regularity Index (Phillips et al. 2017): the
//!   chance of being in the same state, asleep or awake, at the same clock
//!   time on consecutive days, scaled so 100 is perfectly regular and 0 is
//!   random. It is read against the fifths of Windred et al. (*Sleep*
//!   2024), in whose 61,000 adults the risk of dying fell steadily from the
//!   lowest fifth to the highest.
//!
//! **The SRI here is the window's**, from falling asleep to waking, since the
//! window is what the record holds from 2018; Windred's also counts waking in
//! the night. On the operator's nights since 2026-06-01, which record that,
//! the window's reads 2.0 points higher a week on average (0.2 to 5.6), less
//! than half a fifth, so the bands carry over.
//!
//! **The trend is the mean of the 28 nights ending on the day**, at least 12
//! of them recorded, the 3-in-7 that every window here needs: the question is
//! whether the training is being coped with, which is weeks. A day's
//! direction is its trend against the trend 28 days earlier.
//!
//! **One device's nights**, as HRV's are (§ 6); the attribution is
//! [`super::per_device`]'s. A gap is a gap (§ 37): the line breaks wherever
//! fewer than 12 of the 28 nights were recorded.

use std::collections::BTreeMap;

use jiff::{
    SignedDuration,
    civil::{Date, DateTime},
};

use super::{
    body_weight::Direction,
    hrv::Against,
    per_device::{DeviceSeries, Reading},
    range::{Range, runs_within},
};
use crate::measure::PositiveDuration;

/// One night, as both series read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Night {
    /// Garmin's time asleep.
    pub asleep: PositiveDuration,
    /// The wall clock at falling asleep, in the zone in force then.
    pub fell_asleep: DateTime,
    /// The wall clock at waking.
    pub woke: DateTime,
}

/// What each trend is, so a different method is never mistaken for it.
pub const METHOD: &str = "mean of the 28 nights ending on the day, at least 12 of them recorded";

/// The nights a trend is the mean of, the day itself included.
const NIGHTS: i64 = 28;
const RECORDED: usize = 12;

/// The NHS's 7 to 9 hours for adults, in minutes.
pub const NHS: (f64, f64) = (420.0, 540.0);

/// The most time asleep may move in 28 days and still be maintaining
/// (operator, 2026-10-28).
const ASLEEP_MAINTAINING_MINUTES: f64 = 15.0;

/// The lower edge of each of Windred et al.'s fifths above the lowest.
pub const WINDRED: [f64; 4] = [71.6, 78.6, 83.2, 87.3];

/// The most the SRI may move in 28 days and still be maintaining: about half
/// the width of Windred's middle fifths (operator, 2026-10-28).
const REGULARITY_MAINTAINING: f64 = 2.0;

/// Which fifth of Windred et al.'s cohort an SRI falls in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fifth {
    Lowest,
    Second,
    Third,
    Fourth,
    Highest,
}

impl Fifth {
    pub fn of(index: f64) -> Self {
        match WINDRED.iter().filter(|edge| index >= **edge).count() {
            0 => Self::Lowest,
            1 => Self::Second,
            2 => Self::Third,
            3 => Self::Fourth,
            _ => Self::Highest,
        }
    }
}

/// A trend on one day: the 28 nights' mean, and which way it went since 28
/// days before, where that day has a trend too.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trend {
    pub on: Date,
    pub value: f64,
    /// The trend less the trend 28 days earlier.
    pub change: Option<f64>,
    pub direction: Option<Direction>,
}

/// Time asleep, against the NHS's band.
pub fn against_nhs(minutes: f64) -> Against {
    if minutes < NHS.0 {
        Against::Below
    } else if minutes > NHS.1 {
        Against::Above
    } else {
        Against::Within
    }
}

/// Minutes asleep on each morning.
pub fn asleep(readings: &[Reading<Night>]) -> BTreeMap<Date, f64> {
    readings
        .iter()
        .map(|reading| {
            let seconds = u32::try_from(reading.value.asleep.as_seconds()).unwrap_or(u32::MAX);
            (reading.on, f64::from(seconds) / 60.0)
        })
        .collect()
}

/// The SRI of each morning's night against the night before, where both were
/// recorded.
///
/// Two windows on the same clock disagree for the time either covers and the
/// other does not, out of the day's 24 hours. The mean of these over a span is
/// that span's SRI, since every pair of mornings weighs a whole day.
pub fn regularity(readings: &[Reading<Night>]) -> BTreeMap<Date, f64> {
    let nights: BTreeMap<Date, &Night> = readings
        .iter()
        .map(|reading| (reading.on, &reading.value))
        .collect();
    nights
        .iter()
        .filter_map(|(&on, &night)| {
            let before = nights.get(&on.yesterday().ok()?)?;
            Some((on, index(before, night)))
        })
        .collect()
}

/// The SRI of two consecutive nights, each placed on its own morning's clock.
fn index(before: &Night, night: &Night) -> f64 {
    let day = SignedDuration::from_hours(24);
    let (from, to) = (night.fell_asleep - day, night.woke - day);
    let length = |a: DateTime, b: DateTime| b.duration_since(a).as_secs_f64();
    let overlap = length(before.fell_asleep.max(from), before.woke.min(to)).max(0.0);
    let disagreeing = 2.0f64.mul_add(
        -overlap,
        length(before.fell_asleep, before.woke) + length(from, to),
    );
    let same = 1.0 - (disagreeing / day.as_secs_f64()).min(1.0);
    200.0f64.mul_add(same, -100.0)
}

/// The trend through `values`, oldest first, as unbroken runs of consecutive
/// days from the first value to the last, each with its direction where the
/// change beyond `maintaining` can be read.
pub fn trend(values: &BTreeMap<Date, f64>, maintaining: f64) -> Vec<Vec<Trend>> {
    let (Some(&first), Some(&last)) = (values.keys().next(), values.keys().next_back()) else {
        return Vec::new();
    };
    let means: BTreeMap<Date, f64> = first
        .series(jiff::Span::new().days(1))
        .take_while(|on| *on <= last)
        .filter_map(|on| Some((on, mean(values, on)?)))
        .collect();

    let mut runs: Vec<Vec<Trend>> = Vec::new();
    let mut run: Vec<Trend> = Vec::new();
    for on in first
        .series(jiff::Span::new().days(1))
        .take_while(|on| *on <= last)
    {
        let Some(&value) = means.get(&on) else {
            if !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
            continue;
        };
        let change = on
            .checked_sub(jiff::Span::new().days(NIGHTS))
            .ok()
            .and_then(|before| means.get(&before))
            .map(|before| value - before);
        run.push(Trend {
            on,
            value,
            change,
            direction: change.map(|change| {
                if change > maintaining {
                    Direction::Rising
                } else if change < -maintaining {
                    Direction::Falling
                } else {
                    Direction::Maintaining
                }
            }),
        });
    }
    if !run.is_empty() {
        runs.push(run);
    }
    runs
}

/// The mean of the values in the 28 days ending on `on`, if enough of them
/// were recorded.
fn mean(values: &BTreeMap<Date, f64>, on: Date) -> Option<f64> {
    let from = on.checked_sub(jiff::Span::new().days(NIGHTS - 1)).ok()?;
    let within: Vec<f64> = values.range(from..=on).map(|(_, value)| *value).collect();
    if within.len() < RECORDED {
        return None;
    }
    let count = f64::from(u32::try_from(within.len()).ok()?);
    Some(within.iter().sum::<f64>() / count)
}

/// One device's nights and the trends through them.
#[derive(Debug, Clone, PartialEq)]
pub struct Trended {
    pub series: DeviceSeries<Night>,
    /// Each morning's SRI against the night before.
    pub regularity: BTreeMap<Date, f64>,
    /// Time asleep's trend, in minutes.
    pub asleep: Vec<Vec<Trend>>,
    /// The SRI's trend.
    pub regular: Vec<Vec<Trend>>,
}

impl Trended {
    pub fn of(series: DeviceSeries<Night>) -> Self {
        let regularity = regularity(&series.readings);
        let asleep = trend(&asleep(&series.readings), ASLEEP_MAINTAINING_MINUTES);
        let regular = trend(&regularity, REGULARITY_MAINTAINING);
        Self {
            series,
            regularity,
            asleep,
            regular,
        }
    }

    /// Only `range`'s days. The trend on each is the whole record's.
    #[must_use]
    pub fn within(&self, range: Range) -> Self {
        Self {
            series: self.series.within(range),
            regularity: self
                .regularity
                .iter()
                .filter(|(on, _)| range.covers(**on))
                .map(|(on, value)| (*on, *value))
                .collect(),
            asleep: runs_within(&self.asleep, range, |day| day.on),
            regular: runs_within(&self.regular, range, |day| day.on),
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Direction, Fifth, Night, Reading, against_nhs, regularity, trend};
    use crate::{analytical::hrv::Against, measure::PositiveDuration};

    fn night(on: Date, fell: (i8, i8), woke: (i8, i8)) -> Reading<Night> {
        let fell_on = if fell.0 >= 12 {
            on.yesterday().expect("yesterday")
        } else {
            on
        };
        Reading {
            on,
            value: Night {
                asleep: PositiveDuration::from_seconds(25_200).expect("asleep"),
                fell_asleep: fell_on.at(fell.0, fell.1, 0, 0),
                woke: on.at(woke.0, woke.1, 0, 0),
            },
        }
    }

    #[test]
    fn the_same_window_two_nights_running_is_perfectly_regular() {
        let nights = [
            night(date(2026, 10, 12), (23, 0), (7, 0)),
            night(date(2026, 10, 13), (23, 0), (7, 0)),
        ];
        let index = regularity(&nights);
        assert_eq!(index.len(), 1, "the first night has none before it");
        assert!((index[&date(2026, 10, 13)] - 100.0).abs() < 1e-9);
    }

    #[test]
    fn an_hour_later_at_both_ends_disagrees_for_two_hours_of_the_day() {
        // 2026-10-18: fell asleep 00:56 after 23:24 the night before.
        let nights = [
            night(date(2026, 10, 17), (23, 0), (7, 0)),
            night(date(2026, 10, 18), (0, 0), (8, 0)),
        ];
        let expected = 200.0f64.mul_add(1.0 - 2.0 / 24.0, -100.0);
        assert!((regularity(&nights)[&date(2026, 10, 18)] - expected).abs() < 1e-9);
    }

    #[test]
    fn a_night_missing_breaks_the_pair_on_either_side() {
        let nights = [
            night(date(2026, 10, 24), (23, 0), (7, 0)),
            night(date(2026, 10, 25), (23, 0), (7, 0)),
            night(date(2026, 10, 27), (23, 0), (7, 0)),
        ];
        let index = regularity(&nights);
        assert!(index.contains_key(&date(2026, 10, 25)));
        assert!(!index.contains_key(&date(2026, 10, 27)));
    }

    #[test]
    fn the_trend_needs_12_of_28_nights_and_breaks_where_it_has_fewer() {
        let start = date(2026, 1, 1);
        let values = (0..12)
            .map(|day| {
                (
                    start.checked_add(jiff::Span::new().days(day)).expect("day"),
                    420.0,
                )
            })
            .chain((40..60).map(|day| {
                (
                    start.checked_add(jiff::Span::new().days(day)).expect("day"),
                    450.0,
                )
            }))
            .collect();
        let runs = trend(&values, 15.0);
        assert_eq!(runs.len(), 2, "a gap of 28 nights breaks the line");
        assert_eq!(runs[0].first().map(|day| day.on), Some(date(2026, 1, 12)));
    }

    #[test]
    fn the_direction_is_against_the_trend_28_days_earlier() {
        let start = date(2026, 1, 1);
        let values = (0..120)
            .map(|day| {
                let on = start.checked_add(jiff::Span::new().days(day)).expect("day");
                (on, if day < 56 { 420.0 } else { 460.0 })
            })
            .collect();
        let days: Vec<_> = trend(&values, 15.0).into_iter().flatten().collect();
        let on = |on: Date| {
            days.iter()
                .find(|day| day.on == on)
                .expect("a trend that day")
        };

        assert_eq!(
            on(date(2026, 1, 28)).direction,
            None,
            "nothing 28 days before"
        );
        assert_eq!(
            on(date(2026, 2, 25)).direction,
            Some(Direction::Maintaining)
        );
        assert_eq!(on(date(2026, 3, 25)).direction, Some(Direction::Rising));
        assert!((on(date(2026, 3, 25)).change.expect("a change") - 40.0).abs() < 1e-9);
    }

    #[test]
    fn the_bands_are_the_nhs_and_windreds() {
        assert_eq!(against_nhs(419.0), Against::Below);
        assert_eq!(against_nhs(444.0), Against::Within);
        assert_eq!(against_nhs(541.0), Against::Above);
        assert_eq!(Fifth::of(71.5), Fifth::Lowest);
        assert_eq!(Fifth::of(85.0), Fifth::Fourth);
        assert_eq!(Fifth::of(90.3), Fifth::Highest);
    }
}
