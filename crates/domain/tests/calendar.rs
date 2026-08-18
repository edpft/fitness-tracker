//! Where a date sits in a block, when the block is interrupted.
//!
//! **A block's duration counts training weeks.** Days since the start divided by
//! seven counts calendar weeks, and the two differ by every week the operator
//! spent away — which the record shows happening twice in a single block. The
//! difference is not cosmetic: it decides which rung of the ladder a session is
//! prescribed, so a holiday counted as a training week issues a load nobody has
//! climbed to.
//!
//! The dates here are the operator's autumn block: eleven weeks from Monday
//! 2026-09-14, Monday light and Friday heavy.

use domain::prescription::{
    Calendar, InvalidCalendar, NotScheduled, SessionRole, WeekKind, Weekdays,
};
use jiff::{
    civil::{Date, Weekday},
    tz::TimeZone,
};
use proptest::prelude::*;

// Strategy and fixture helpers are free functions, so the test exemptions do not
// reach them and they may not panic.

fn date(year: i16, month: i8, day: i8) -> Result<Date, Box<dyn std::error::Error>> {
    Ok(Date::new(year, month, day)?)
}

fn monday_light_friday_heavy() -> Result<Weekdays, Box<dyn std::error::Error>> {
    Ok(Weekdays::new(vec![
        (Weekday::Monday, SessionRole::Light),
        (Weekday::Friday, SessionRole::Heavy),
    ])?)
}

/// Eleven training weeks from 2026-09-14, skipping the weeks named.
fn autumn(skipping: &[Date]) -> Result<Calendar, Box<dyn std::error::Error>> {
    Ok(Calendar::new(
        date(2026, 9, 14)?,
        11,
        skipping,
        monday_light_friday_heavy()?,
        TimeZone::UTC,
    )?)
}

/// The training week a date falls in, one-based, with the test week as its
/// number rather than a variant — enough to compare two calendars.
fn week_of(calendar: &Calendar, on: Date) -> Option<u32> {
    match calendar.place(on).ok()? {
        (WeekKind::Climbing(week), _) => Some(week.as_u32()),
        (WeekKind::Test, _) => Some(calendar.duration_weeks()),
    }
}

/// A week away is not a week of the plan.
///
/// The session after the holiday gets the rung the session before it would have
/// led to. Asserted against the same block without the interruption, because the
/// claim is about the *difference*: an absolute expectation would pass just as
/// well against a calendar that had shifted everything by one.
#[test]
fn a_holiday_does_not_advance_the_ladder() {
    let (Ok(away), Ok(uninterrupted), Ok(before), Ok(after)) = (
        date(2026, 10, 12),
        autumn(&[]),
        date(2026, 10, 5),
        date(2026, 10, 19),
    ) else {
        panic!("the block and its dates are all valid")
    };
    let Ok(interrupted) = autumn(&[away]) else {
        panic!("a week inside the block can be skipped")
    };

    // The week before the holiday is the same week in both.
    assert_eq!(week_of(&interrupted, before), Some(4));
    assert_eq!(week_of(&uninterrupted, before), Some(4));

    // The week after it is the *next* rung, not the one after that.
    assert_eq!(
        week_of(&interrupted, after),
        Some(5),
        "the week after a holiday climbs one rung, not two"
    );
    assert_eq!(
        week_of(&uninterrupted, after),
        Some(6),
        "and it would have been two, which is the bug"
    );
}

/// An interrupted week has its programmed weekdays and no sessions.
///
/// Refused rather than answered with the neighbouring week's loading: the
/// operator is on a beach, and a prescription for a day they will not train is
/// worse than none.
#[test]
fn an_interrupted_week_issues_nothing() {
    let (Ok(away), Ok(friday)) = (date(2026, 10, 12), date(2026, 10, 16)) else {
        panic!("the dates are valid")
    };
    let Ok(calendar) = autumn(&[away]) else {
        panic!("a week inside the block can be skipped")
    };

    for day in [away, friday] {
        match calendar.place(day) {
            Err(NotScheduled::Interrupted { date, week }) => {
                assert_eq!(date, day);
                assert_eq!(week, away, "the refusal names the week as authored");
            }
            other => panic!("{day} is in a skipped week, got {other:?}"),
        }
    }
}

/// The block still ends after its eleventh *training* week, one calendar week
/// later than it would have.
#[test]
fn an_interruption_moves_the_test_week_out_rather_than_dropping_it() {
    let (Ok(away), Ok(was_the_test), Ok(is_the_test), Ok(past_the_end)) = (
        date(2026, 10, 12),
        date(2026, 11, 23),
        date(2026, 11, 30),
        date(2026, 12, 7),
    ) else {
        panic!("the dates are valid")
    };
    let Ok(calendar) = autumn(&[away]) else {
        panic!("a week inside the block can be skipped")
    };

    assert_eq!(calendar.duration_weeks(), 11, "eleven weeks of training");
    assert_eq!(
        calendar.calendar_weeks(),
        12,
        "over twelve weeks of calendar"
    );

    assert!(
        matches!(calendar.place(was_the_test), Ok((WeekKind::Climbing(_), _))),
        "what would have been the test is now the last climbing week"
    );
    assert!(matches!(
        calendar.place(is_the_test),
        Ok((WeekKind::Test, SessionRole::Light))
    ));
    assert!(matches!(
        calendar.place(past_the_end),
        Err(NotScheduled::PastEnd { .. })
    ));
}

/// The default `--date` is the next *session*, and an interrupted week has none.
///
/// A search over one week of weekdays was enough before and is not now: a
/// holiday week has both its programmed days, so a weekday-only search returns a
/// date that then refuses to be placed.
#[test]
fn the_next_session_steps_over_a_holiday() {
    let (Ok(away), Ok(after), Ok(last_session)) =
        (date(2026, 10, 12), date(2026, 10, 19), date(2026, 12, 4))
    else {
        panic!("the dates are valid")
    };
    let Ok(calendar) = autumn(&[away]) else {
        panic!("a week inside the block can be skipped")
    };

    assert_eq!(calendar.next_programmed(away), Some(after));
    assert_eq!(
        calendar.next_programmed(last_session),
        Some(last_session),
        "on a training day, today is the answer"
    );
    let Ok(over) = date(2026, 12, 7) else {
        panic!("the date is valid")
    };
    assert_eq!(
        calendar.next_programmed(over),
        None,
        "once the block is over there is no next session"
    );
}

/// Two dates in one week are one interruption, not two.
///
/// The operator names a week by a date inside it, and a holiday spanning a
/// weekend is easy to name twice. Counting it twice would take a rung off the
/// block.
#[test]
fn two_dates_in_one_week_name_one_interruption() {
    let (Ok(monday), Ok(thursday), Ok(after)) =
        (date(2026, 10, 12), date(2026, 10, 15), date(2026, 10, 19))
    else {
        panic!("the dates are valid")
    };
    let Ok(calendar) = autumn(&[monday, thursday]) else {
        panic!("both dates are inside the block")
    };

    assert_eq!(calendar.interruptions().len(), 1);
    assert_eq!(calendar.calendar_weeks(), 12);
    assert_eq!(week_of(&calendar, after), Some(5));
}

/// An interruption outside the block is refused, not ignored.
///
/// It changes nothing either way. What it means is that the operator and the
/// programme disagree about when the block runs, and that is worth more than the
/// holiday.
#[test]
fn an_interruption_outside_the_block_is_refused() {
    // Built here rather than through `autumn`, so the error type is the one
    // being asserted about rather than a boxed one.
    let (Ok(start), Ok(weekdays), Ok(before), Ok(after)) = (
        date(2026, 9, 14),
        monday_light_friday_heavy(),
        date(2026, 9, 7),
        date(2026, 12, 7),
    ) else {
        panic!("the fixture is valid")
    };
    let block = |weeks: &[Date]| {
        Calendar::new(start, 11, weeks, weekdays.clone(), TimeZone::UTC).map(|_| ())
    };

    assert!(matches!(
        block(&[before]),
        Err(InvalidCalendar::InterruptionBeforeStart { .. })
    ));
    assert!(matches!(
        block(&[after]),
        Err(InvalidCalendar::InterruptionPastEnd { .. })
    ));
}

/// A block of no weeks issues nothing, and says so at construction.
#[test]
fn a_block_of_no_weeks_is_not_a_block() {
    let (Ok(start), Ok(weekdays)) = (date(2026, 9, 14), monday_light_friday_heavy()) else {
        panic!("the fixture is valid")
    };
    assert_eq!(
        Calendar::new(start, 0, &[], weekdays, TimeZone::UTC),
        Err(InvalidCalendar::NoWeeks)
    );
}

fn a_date() -> impl Strategy<Value = Date> {
    (2020_i16..2030, 1_i8..=12, 1_i8..=28)
        .prop_filter_map("a valid calendar date", |(year, month, day)| {
            Date::new(year, month, day).ok()
        })
}

/// An arbitrary block, with up to three of its own weeks skipped.
///
/// The skipped weeks are drawn from inside the duration, which is where a week
/// the block skips can be: past that, the block has already finished (§ 28 —
/// the generator builds through the real constructor and never around it).
fn a_calendar() -> impl Strategy<Value = Calendar> {
    (a_date(), 1_u32..=20)
        .prop_flat_map(|(start, duration)| {
            (
                Just(start),
                Just(duration),
                prop::collection::btree_set(0..duration, 0..4),
            )
        })
        .prop_filter_map("a block its own rules accept", |(start, duration, away)| {
            let weeks: Vec<Date> = away
                .iter()
                .filter_map(|offset| {
                    start
                        .checked_add(jiff::Span::new().weeks(i64::from(*offset)))
                        .ok()
                })
                .collect();
            Calendar::new(
                start,
                duration,
                &weeks,
                monday_light_friday_heavy().ok()?,
                TimeZone::UTC,
            )
            .ok()
        })
}

proptest! {
    /// Whatever the interruptions, the block runs each training week once and
    /// runs them in order.
    ///
    /// Every day of the calendar span is offered, and the sessions that come
    /// back must be exactly two per training week — a Monday and a Friday, since
    /// every week contains one of each — numbered one to the duration, ascending.
    /// That is the property a holiday must not disturb, stated without reference
    /// to which weeks were skipped.
    #[test]
    fn a_block_runs_every_training_week_once_and_in_order(calendar in a_calendar()) {
        let span = i64::from(calendar.calendar_weeks()) * 7;
        let mut weeks = Vec::new();
        for day in 0..span {
            let Ok(date) = calendar.start().checked_add(jiff::Span::new().days(day)) else {
                continue;
            };
            if let Some(week) = week_of(&calendar, date) {
                weeks.push(week);
            }
        }

        let mut expected = Vec::new();
        for week in 1..=calendar.duration_weeks() {
            expected.push(week);
            expected.push(week);
        }
        prop_assert_eq!(weeks, expected);
    }

    /// A day in a skipped week is never a session, and every other placeable day
    /// carries a week the ladder has.
    #[test]
    fn a_skipped_week_holds_no_session(calendar in a_calendar()) {
        for week in calendar.interruptions().iter() {
            for day in 0..7_i64 {
                let Ok(date) = week.checked_add(jiff::Span::new().days(day)) else {
                    continue;
                };
                prop_assert!(
                    matches!(calendar.place(date), Err(NotScheduled::Interrupted { .. }))
                        || matches!(calendar.place(date), Err(NotScheduled::NotAProgrammedDay { .. })),
                    "{date} is in a skipped week and cannot be a session"
                );
            }
        }
    }
}
