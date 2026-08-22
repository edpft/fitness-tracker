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

use std::num::NonZeroU8;

use domain::prescription::{
    Calendar, InvalidCalendar, NotScheduled, SessionRole, Skip, WeekKind, Weekdays,
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

/// Eleven training weeks from 2026-09-14, skipping what it is given.
fn autumn(skipping: &[Skip]) -> Result<Calendar, Box<dyn std::error::Error>> {
    Ok(Calendar::new(
        date(2026, 9, 14)?,
        11,
        skipping,
        monday_light_friday_heavy()?,
        TimeZone::UTC,
    )?)
}

/// A whole week away, as a seven-day skip from the Monday named.
///
/// The old model took a single date and skipped its whole week. That is now one
/// case of a range rather than the only thing expressible, and these tests say
/// so explicitly.
fn week_from(monday: Date) -> Result<Skip, Box<dyn std::error::Error>> {
    Ok(Skip::new(
        monday,
        NonZeroU8::new(7).ok_or("seven is not zero")?,
    ))
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
    let Ok(week) = week_from(away) else {
        panic!("seven days is a skip")
    };
    let Ok(interrupted) = autumn(&[week]) else {
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
    let Ok(week) = week_from(away) else {
        panic!("seven days is a skip")
    };
    let Ok(calendar) = autumn(&[week]) else {
        panic!("a week inside the block can be skipped")
    };

    for day in [away, friday] {
        match calendar.place(day) {
            Err(NotScheduled::Interrupted { date, skip }) => {
                assert_eq!(date, day);
                assert_eq!(skip.start(), away, "the refusal names the skip as authored");
            }
            other => panic!("{day} is in a skipped week, got {other:?}"),
        }
    }
}

/// The block still ends after its eleventh *training* week, one calendar week
/// later than it would have.
///
/// **Every week of it climbs** (decision 0013). This used to assert that the
/// last week was a test and that an interruption pushed that test out; a linear
/// programme has no test now, so what moves is the last *climbing* week. The
/// claim is unchanged — an interruption costs a calendar week and not a rung.
#[test]
fn an_interruption_moves_the_last_week_out_rather_than_dropping_it() {
    let (Ok(away), Ok(tenth), Ok(eleventh), Ok(past_the_end)) = (
        date(2026, 10, 12),
        date(2026, 11, 23),
        date(2026, 11, 30),
        date(2026, 12, 7),
    ) else {
        panic!("the dates are valid")
    };
    let Ok(week) = week_from(away) else {
        panic!("seven days is a skip")
    };
    let (Ok(interrupted), Ok(uninterrupted)) = (autumn(&[week]), autumn(&[])) else {
        panic!("a week inside the block can be skipped")
    };

    assert_eq!(interrupted.duration_weeks(), 11, "eleven weeks of training");
    assert_eq!(
        interrupted.calendar_weeks(),
        12,
        "over twelve weeks of calendar"
    );

    // The claim is about the difference: uninterrupted, the block is over by
    // the 30th; interrupted, that week is its eleventh and last.
    assert_eq!(week_of(&uninterrupted, tenth), Some(11));
    assert!(matches!(
        uninterrupted.place(eleventh),
        Err(NotScheduled::PastEnd { .. })
    ));

    assert_eq!(
        week_of(&interrupted, tenth),
        Some(10),
        "the week away cost a calendar week, not a rung"
    );
    assert_eq!(week_of(&interrupted, eleventh), Some(11));
    assert!(matches!(
        interrupted.place(past_the_end),
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
    let Ok(week) = week_from(away) else {
        panic!("seven days is a skip")
    };
    let Ok(calendar) = autumn(&[week]) else {
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

/// A week that loses some of its sessions is still a training week.
///
/// **This test used to assert the opposite**, because an interruption named a
/// whole week: two dates inside one week were deduped to one, and that week ran
/// nothing. Skipping the Monday now leaves the Friday, so the week is a week of
/// the block, the ladder advances through it, and the calendar is no longer
/// one week longer. That is the operator's rule from 2026-08-21 — a week is a
/// training week if at least one of its sessions survives — and it is what lets
/// "away Friday, back for Monday" be said at all.
#[test]
fn a_week_that_keeps_one_session_is_still_a_training_week() {
    let (Ok(monday), Ok(friday), Ok(after)) =
        (date(2026, 10, 12), date(2026, 10, 16), date(2026, 10, 19))
    else {
        panic!("the dates are valid")
    };
    let Ok(calendar) = autumn(&[Skip::day(monday)]) else {
        panic!("one day inside the block can be skipped")
    };

    assert_eq!(calendar.interruptions().len(), 1);
    assert_eq!(
        calendar.calendar_weeks(),
        11,
        "the block is not extended, because the week still ran"
    );
    assert_eq!(week_of(&calendar, friday), Some(5), "the Friday still runs");
    assert_eq!(week_of(&calendar, after), Some(6));
}

/// A week that loses every session is not a training week, and extends the block.
///
/// The old whole-week behaviour, now a consequence rather than the only thing
/// expressible.
#[test]
fn a_week_that_keeps_no_session_extends_the_block() {
    let (Ok(monday), Ok(after)) = (date(2026, 10, 12), date(2026, 10, 19)) else {
        panic!("the dates are valid")
    };
    let Ok(week) = week_from(monday) else {
        panic!("seven days is a skip")
    };
    let Ok(calendar) = autumn(&[week]) else {
        panic!("a week inside the block can be skipped")
    };

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
    let block = |skips: &[Skip]| {
        Calendar::new(start, 11, skips, weekdays.clone(), TimeZone::UTC).map(|_| ())
    };

    assert!(matches!(
        block(&[Skip::day(before)]),
        Err(InvalidCalendar::InterruptionBeforeStart { .. })
    ));
    assert!(matches!(
        block(&[Skip::day(after)]),
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

/// An arbitrary block, with up to three of its own weeks skipped whole.
///
/// Whole weeks rather than single days, because the properties below are about
/// how a block absorbs a week away. Skips drawn from inside the duration, which
/// is where a week the block skips can be: past that, the block has already
/// finished (§ 28 — the generator builds through the real constructor and never
/// around it).
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
            let seven = NonZeroU8::new(7)?;
            let weeks: Vec<Skip> = away
                .iter()
                .filter_map(|offset| {
                    start
                        .checked_add(jiff::Span::new().weeks(i64::from(*offset)))
                        .ok()
                        .map(|monday| Skip::new(monday, seven))
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

    /// A skipped day is never a session, whatever else the week holds.
    #[test]
    fn a_skipped_week_holds_no_session(calendar in a_calendar()) {
        for skip in calendar.interruptions().iter() {
            for day in 0..i64::from(skip.days().get()) {
                let Ok(date) = skip.start().checked_add(jiff::Span::new().days(day)) else {
                    continue;
                };
                prop_assert!(
                    matches!(calendar.place(date), Err(NotScheduled::Interrupted { .. }))
                        || matches!(calendar.place(date), Err(NotScheduled::NotAProgrammedDay { .. })),
                    "{date} is skipped and cannot be a session"
                );
            }
        }
    }
}
