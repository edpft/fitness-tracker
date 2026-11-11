//! The training done, and how much of it a day still carries (#517, #549).
//!
//! **Every hour of it counts alike** (operator, 2026-11-11: *"yes, all
//! activity hours"*): a gym session's, a cycling session's and a padel
//! game's.
//!
//! **Training carried forward and fading** is the standard account (Morton,
//! Fitz-Clarke & Banister 1990): each session produces a response that decays
//! exponentially once training stops, and repeated sessions add up. What a
//! day carries here is the exponentially weighted mean of the hours trained
//! on the days before it.
//!
//! **A time constant of 14 days**, which is an agent's choice the operator
//! has not ruled on. On his record resting heart rate and overnight HRV
//! follow the training carried best at 14 to 21 days and stress at 7 to 14,
//! none of them sharply, and 21 moves the expected resting heart rate over
//! autumn 2026 by 0.1 bpm.

use std::collections::BTreeMap;

use jiff::civil::Date;

use crate::measure::PositiveDuration;

/// How many days it takes what a day carries to fade to 1/e of itself.
pub const TIME_CONSTANT_DAYS: f64 = 14.0;

/// What a session of training was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    Gym,
    Cycling,
    Padel,
}

/// One session of training done: the day it started on, where it was
/// performed, and how long it took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    pub on: Date,
    pub kind: Kind,
    pub length: PositiveDuration,
}

/// The hours trained on each day that held any.
pub fn hours(sessions: &[Session]) -> BTreeMap<Date, f64> {
    let mut seconds: BTreeMap<Date, u64> = BTreeMap::new();
    for session in sessions {
        let day = seconds.entry(session.on).or_default();
        *day = day.saturating_add(session.length.as_seconds());
    }
    seconds
        .into_iter()
        .map(|(on, seconds)| {
            (
                on,
                f64::from(u32::try_from(seconds).unwrap_or(u32::MAX)) / 3600.0,
            )
        })
        .collect()
}

/// The training each day carries, in hours a day.
#[derive(Debug, Clone, PartialEq)]
pub struct Carried {
    days: BTreeMap<Date, f64>,
}

impl Carried {
    /// From the first day trained through `to`.
    ///
    /// **A day carries what came before it and not its own session**, so a
    /// reading taken that morning is set against training already done.
    pub fn through(sessions: &[Session], to: Date) -> Self {
        let hours = hours(sessions);
        let Some(&first) = hours.keys().next() else {
            return Self {
                days: BTreeMap::new(),
            };
        };
        let keep = (-1.0 / TIME_CONSTANT_DAYS).exp();
        let mut level = 0.0_f64;
        let days = first
            .series(jiff::Span::new().days(1))
            .take_while(|on| *on <= to)
            .map(|on| {
                let carried = level;
                level = level.mul_add(keep, hours.get(&on).copied().unwrap_or(0.0) * (1.0 - keep));
                (on, carried)
            })
            .collect();
        Self { days }
    }

    /// What `on` carries; `None` before the first day trained and after the
    /// last day read through.
    pub fn on(&self, on: Date) -> Option<f64> {
        self.days.get(&on).copied()
    }

    /// The mean of what the `length` days ending on `last` carry; `None`
    /// unless every one of them carries a figure.
    pub fn mean(&self, last: Date, length: i64) -> Option<f64> {
        let from = last.checked_sub(jiff::Span::new().days(length - 1)).ok()?;
        let carried: Vec<f64> = self
            .days
            .range(from..=last)
            .map(|(_, value)| *value)
            .collect();
        (i64::try_from(carried.len()).ok()? == length)
            .then(|| super::composition::mean(&carried))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Carried, Kind, Session, TIME_CONSTANT_DAYS, hours};
    use crate::measure::PositiveDuration;

    fn session(on: Date, kind: Kind, minutes: u64) -> Session {
        Session {
            on,
            kind,
            length: PositiveDuration::from_seconds(minutes * 60).expect("a duration"),
        }
    }

    #[test]
    fn a_days_hours_are_every_sessions_whatever_it_was() {
        let day = date(2026, 10, 7);
        let hours = hours(&[
            session(day, Kind::Cycling, 45),
            session(day, Kind::Cycling, 5),
            session(day, Kind::Gym, 100),
            session(date(2026, 10, 11), Kind::Padel, 120),
        ]);
        assert_eq!(hours.get(&day).copied(), Some(2.5));
        assert_eq!(hours.get(&date(2026, 10, 11)).copied(), Some(2.0));
    }

    #[test]
    fn a_day_carries_what_came_before_it_and_not_its_own_session() {
        let first = date(2026, 10, 5);
        let carried = Carried::through(&[session(first, Kind::Gym, 60)], date(2026, 10, 7));
        assert_eq!(carried.on(first), Some(0.0));
        let keep = (-1.0 / TIME_CONSTANT_DAYS).exp();
        let next = carried.on(date(2026, 10, 6)).expect("the day after");
        assert!((next - (1.0 - keep)).abs() < 1e-12, "{next}");
        assert_eq!(carried.on(date(2026, 10, 4)), None);
        assert_eq!(carried.on(date(2026, 10, 8)), None);
    }

    #[test]
    fn training_every_day_carries_its_daily_hours_and_stopping_fades_it() {
        let first = date(2026, 1, 1);
        let sessions: Vec<Session> = first
            .series(jiff::Span::new().days(1))
            .take(200)
            .map(|on| session(on, Kind::Gym, 60))
            .collect();
        let carried = Carried::through(&sessions, date(2026, 8, 31));
        let trained = carried.on(date(2026, 7, 19)).expect("the last day trained");
        assert!((trained - 1.0).abs() < 1e-3, "{trained}");
        let fortnight = carried.on(date(2026, 8, 3)).expect("two weeks after");
        assert!(
            (fortnight / trained - (-1.0_f64).exp()).abs() < 0.03,
            "{fortnight}"
        );
    }

    #[test]
    fn a_mean_needs_every_day_of_its_window() {
        let first = date(2026, 10, 5);
        let carried = Carried::through(&[session(first, Kind::Gym, 60)], date(2026, 10, 20));
        assert!(carried.mean(date(2026, 10, 18), 14).is_some());
        assert_eq!(carried.mean(date(2026, 10, 17), 14), None);
    }
}
