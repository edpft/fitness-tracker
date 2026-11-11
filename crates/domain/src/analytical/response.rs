//! A reading that answers training, set against the expected result of the
//! training done (#517).
//!
//! The operator, 2026-11-11: *"am I responding to training, physiologically,
//! in the way we would expect?"* Resting heart rate, overnight HRV and stress
//! are read this way. **A response reading is read against the training done,
//! never against a date alone**: *"it's not because I expect to arbitrarily be
//! getting fitter, it's because I know I have been training and I'm hoping to
//! see the expected result."*
//!
//! **Since the macrocycle's intended start, to today** (operator, the same
//! day). The start is the date the plan holds, whatever happened to the entry
//! tests, and the reading there is the mean of the two weeks before it. Now is
//! the mean of the two weeks to today.
//!
//! **The expected result is a line through his own weeks**: the reading
//! regressed on the training carried ([`super::training_done`]), week by
//! week, over every week one watch recorded from four weeks after its first
//! day. The expected change is that line's slope times the change in the
//! training carried. Both are the mocks' the operator read and signed off;
//! he has ruled on neither by itself.
//!
//! **Nothing here shows how loose the expectation is.** On the Forerunner
//! 165's 93 weeks the line fits at r of about 0.5, and the scatter about it is
//! as large as the changes it expects (#549, 2026-10-16).
//!
//! **Each week since, too**: the mean of the week to each day, its noise, and
//! where the line puts it (operator: *"is it possible to compare all weeks in
//! the macrocycle against expectation?"*).
//!
//! **The noise is [`super::confidence`]'s**, of each window's mean, and a
//! change's is the two windows' combined. A week's is drawn against the two
//! weeks before the start, since the expected result is.
//!
//! **Each window needs 3 in 7 of its days recorded**, as every window here
//! does: 6 of the 14, 3 of the 7.

use std::collections::BTreeMap;

use jiff::civil::Date;

use super::{composition::mean, confidence::Noise, range::Range, training_done::Carried};

const FORTNIGHT_DAYS: i64 = 14;
const FORTNIGHT_RECORDED: usize = 6;
const WEEK_DAYS: i64 = 7;
const WEEK_RECORDED: usize = 3;

/// How long after a watch's first day its weeks are read from.
const SETTLING_DAYS: i64 = 28;
/// How many days of a week have to be recorded for it to be one of the weeks
/// the line is drawn through.
const LINE_WEEK_RECORDED: usize = 4;

/// How a reading has moved with the training carried, on one watch's record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Expected {
    /// The reading's change for each hour a day of training carried.
    per_hour: f64,
}

impl Expected {
    /// The line through each week's mean reading against its mean training
    /// carried. `None` with fewer than three such weeks, or where the
    /// training carried never changed.
    pub fn of(days: &BTreeMap<Date, f64>, carried: &Carried) -> Option<Self> {
        let first = *days.keys().next()?;
        let from = first
            .checked_add(jiff::Span::new().days(SETTLING_DAYS))
            .ok()?;
        let mut weeks: BTreeMap<Date, (Vec<f64>, Vec<f64>)> = BTreeMap::new();
        for (&on, &read) in days.range(from..) {
            let Some(training) = carried.on(on) else {
                continue;
            };
            let monday = on
                .checked_sub(
                    jiff::Span::new().days(i64::from(on.weekday().to_monday_zero_offset())),
                )
                .ok()?;
            let week = weeks.entry(monday).or_default();
            week.0.push(training);
            week.1.push(read);
        }
        let points: Vec<(f64, f64)> = weeks
            .values()
            .filter(|(_, read)| read.len() >= LINE_WEEK_RECORDED)
            .filter_map(|(training, read)| Some((mean(training)?, mean(read)?)))
            .collect();
        if points.len() < 3 {
            return None;
        }
        let training = mean(&points.iter().map(|(x, _)| *x).collect::<Vec<_>>())?;
        let read = mean(&points.iter().map(|(_, y)| *y).collect::<Vec<_>>())?;
        let spread: f64 = points.iter().map(|(x, _)| (x - training).powi(2)).sum();
        let together: f64 = points
            .iter()
            .map(|(x, y)| (x - training) * (y - read))
            .sum();
        (spread > 0.0).then(|| Self {
            per_hour: together / spread,
        })
    }

    /// The change the line expects for a change in the training carried.
    fn change(self, carried: f64) -> f64 {
        self.per_hour * carried
    }
}

/// The two weeks to today against the two weeks before the start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Change {
    /// The days now is read over.
    pub over: Range,
    /// The mean of the two weeks to today.
    pub now: f64,
    /// Now less the mean of the two weeks before the start.
    pub by: f64,
    /// How far either side of `by` its 95% band runs; `None` where the record
    /// is too short to measure the noise.
    pub margin: Option<f64>,
    /// The change the training done expects; `None` where there is no line,
    /// or the training carried is not known over both windows.
    pub expected: Option<f64>,
}

/// The week to one day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Week {
    pub to: Date,
    pub read: f64,
    /// How far either side its 95% band runs, against the two weeks before
    /// the start.
    pub margin: Option<f64>,
    /// Where the training done expects it.
    pub expected: Option<f64>,
}

/// One reading over a macrocycle so far.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    /// The mean of the two weeks before the start, which everything here is
    /// read against.
    pub before: f64,
    /// `None` where the two weeks to today hold too few days.
    pub change: Option<Change>,
    /// The week to each day since the start, oldest first, as unbroken runs.
    pub weeks: Vec<Vec<Week>>,
}

/// One watch's `days` of a reading over the macrocycle that `began` on a day,
/// through `today`. `None` where the two weeks before it began hold too few
/// days to read anything against.
pub fn response(
    days: &BTreeMap<Date, f64>,
    carried: &Carried,
    began: Date,
    today: Date,
) -> Option<Response> {
    let eve = began.yesterday().ok()?;
    let before = window(days, eve, FORTNIGHT_DAYS, FORTNIGHT_RECORDED)?;
    let line = Expected::of(days, carried);
    let carried_before = carried.mean(eve, FORTNIGHT_DAYS);
    let expected =
        |last: Date, length: i64| Some(line?.change(carried.mean(last, length)? - carried_before?));
    let fortnight = Noise::of(days, FORTNIGHT_DAYS, FORTNIGHT_RECORDED);
    let week = Noise::of(days, WEEK_DAYS, WEEK_RECORDED);
    let margin_before = fortnight.as_ref().and_then(|noise| noise.margin(days, eve));

    let change = window(days, today, FORTNIGHT_DAYS, FORTNIGHT_RECORDED).and_then(|now| {
        Some(Change {
            over: Range {
                from: today
                    .checked_sub(jiff::Span::new().days(FORTNIGHT_DAYS - 1))
                    .ok()?,
                to: today,
            },
            now,
            by: now - before,
            margin: fortnight
                .as_ref()
                .and_then(|noise| noise.margin(days, today))
                .zip(margin_before)
                .map(|(now, before)| now.hypot(before)),
            expected: expected(today, FORTNIGHT_DAYS),
        })
    });

    let mut weeks: Vec<Vec<Week>> = Vec::new();
    let mut run: Vec<Week> = Vec::new();
    let first = began
        .checked_add(jiff::Span::new().days(WEEK_DAYS - 1))
        .ok()?;
    for to in first
        .series(jiff::Span::new().days(1))
        .take_while(|to| *to <= today)
    {
        match window(days, to, WEEK_DAYS, WEEK_RECORDED) {
            Some(read) => run.push(Week {
                to,
                read,
                margin: week
                    .as_ref()
                    .and_then(|noise| noise.margin(days, to))
                    .zip(margin_before)
                    .map(|(week, before)| week.hypot(before)),
                expected: expected(to, WEEK_DAYS).map(|by| before + by),
            }),
            None if run.is_empty() => {}
            None => weeks.push(std::mem::take(&mut run)),
        }
    }
    if !run.is_empty() {
        weeks.push(run);
    }

    Some(Response {
        before,
        change,
        weeks,
    })
}

/// The mean of the `length` days ending on `last`, where at least `recorded`
/// of them have a figure.
fn window(days: &BTreeMap<Date, f64>, last: Date, length: i64, recorded: usize) -> Option<f64> {
    let from = last.checked_sub(jiff::Span::new().days(length - 1)).ok()?;
    let values: Vec<f64> = days.range(from..=last).map(|(_, value)| *value).collect();
    (values.len() >= recorded).then(|| mean(&values)).flatten()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use jiff::civil::{Date, date};

    use super::{Expected, response};
    use crate::{
        analytical::training_done::{Carried, Kind, Session},
        measure::PositiveDuration,
    };

    const FIRST: Date = date(2025, 1, 1);
    const BEGAN: Date = date(2026, 9, 14);
    const TODAY: Date = date(2026, 11, 11);

    /// An hour a day in alternate eight-week blocks, from a year before the
    /// reading starts, and an hour every day of the macrocycle.
    fn trained() -> Vec<Session> {
        date(2024, 1, 1)
            .series(jiff::Span::new().days(1))
            .take_while(|on| *on <= TODAY)
            .enumerate()
            .filter(|(index, on)| (index / 56) % 2 == 0 || *on >= BEGAN)
            .map(|(_, on)| Session {
                on,
                kind: Kind::Gym,
                length: PositiveDuration::from_seconds(3600).expect("a duration"),
            })
            .collect()
    }

    /// A reading that sits at 60 and falls 10 for every hour a day carried,
    /// with a beat of day-to-day noise.
    fn reading(carried: &Carried, by: impl Fn(Date) -> f64) -> BTreeMap<Date, f64> {
        FIRST
            .series(jiff::Span::new().days(1))
            .take_while(|on| *on <= TODAY)
            .enumerate()
            .filter_map(|(index, on)| {
                let wobble = if index % 2 == 0 { 0.5 } else { -0.5 };
                Some((
                    on,
                    10.0_f64.mul_add(-carried.on(on)?, 60.0) + wobble + by(on),
                ))
            })
            .collect()
    }

    #[test]
    fn the_line_is_how_the_reading_has_moved_with_the_training_carried() {
        let carried = Carried::through(&trained(), TODAY);
        let days = reading(&carried, |_| 0.0);
        let line = Expected::of(&days, &carried).expect("a line");
        assert!((line.per_hour + 10.0).abs() < 0.2, "{}", line.per_hour);
    }

    #[test]
    fn a_reading_that_follows_the_training_sits_on_its_expected_result() {
        let carried = Carried::through(&trained(), TODAY);
        let days = reading(&carried, |_| 0.0);
        let response = response(&days, &carried, BEGAN, TODAY).expect("a response");
        let change = response.change.expect("two weeks to today");
        let expected = change.expected.expect("an expected result");
        assert!(change.by < -1.0, "{}", change.by);
        assert!((change.by - expected).abs() < 0.3, "{change:?}");
        assert_eq!(change.over.from, date(2026, 10, 29));
        let weeks: Vec<_> = response.weeks.iter().flatten().collect();
        assert_eq!(weeks.first().map(|week| week.to), Some(date(2026, 9, 20)));
        assert_eq!(weeks.last().map(|week| week.to), Some(TODAY));
        for week in weeks {
            let expected = week.expected.expect("an expected result");
            assert!((week.read - expected).abs() < 0.5, "{week:?}");
        }
    }

    #[test]
    fn a_reading_that_went_the_other_way_is_beyond_its_noise_of_the_expected_result() {
        let carried = Carried::through(&trained(), TODAY);
        let days = reading(&carried, |on| if on >= BEGAN { 12.0 } else { 0.0 });
        let response = response(&days, &carried, BEGAN, TODAY).expect("a response");
        let change = response.change.expect("two weeks to today");
        let margin = change.margin.expect("a margin");
        let expected = change.expected.expect("an expected result");
        assert!(change.by > 0.0 && expected < 0.0, "{change:?}");
        assert!(change.by - expected > margin, "{change:?}");
    }

    #[test]
    fn nothing_recorded_before_the_start_reads_nothing() {
        let carried = Carried::through(&trained(), TODAY);
        let days: BTreeMap<Date, f64> = reading(&carried, |_| 0.0)
            .into_iter()
            .filter(|(on, _)| *on >= BEGAN)
            .collect();
        assert_eq!(response(&days, &carried, BEGAN, TODAY), None);
    }

    #[test]
    fn a_week_with_too_few_days_breaks_the_run() {
        let carried = Carried::through(&trained(), TODAY);
        let days: BTreeMap<Date, f64> = reading(&carried, |_| 0.0)
            .into_iter()
            .filter(|(on, _)| *on < date(2026, 10, 1) || *on > date(2026, 10, 14))
            .collect();
        let response = response(&days, &carried, BEGAN, TODAY).expect("a response");
        assert_eq!(response.weeks.len(), 2, "{:?}", response.weeks);
    }
}
