//! School and public holidays, on the operator's real dates for the autumn of
//! 2026 (#181).

use std::num::NonZeroU8;

use domain::schedule::{Holidays, PublicHoliday, SchoolHoliday};
use jiff::civil::date;

/// Half term and Christmas 2026 as the school publishes them, its calendar
/// reaching to the end of the Christmas holiday, and Christmas and Boxing Day
/// as gov.uk publishes them, to the end of 2028. `None` only if a length were
/// zero, which neither is.
fn christmas() -> Option<Holidays> {
    let school = Holidays::from_school(
        vec![
            SchoolHoliday::new(date(2026, 12, 21), NonZeroU8::new(14)?),
            SchoolHoliday::new(date(2026, 10, 26), NonZeroU8::new(5)?),
        ],
        Some(date(2027, 1, 3)),
    );
    let public = Holidays::from_public(
        vec![
            PublicHoliday::new(date(2026, 12, 28), "Boxing Day".to_owned()),
            PublicHoliday::new(date(2026, 12, 25), "Christmas Day".to_owned()),
        ],
        Some(date(2028, 12, 26)),
    );
    Some(school.and(public))
}

/// Christmas week touches the school holiday, and the week before does not.
#[test]
fn christmas_week_is_touched_and_the_week_before_is_not() {
    let holidays = christmas().expect("the holidays build");

    assert_eq!(
        holidays.touches(date(2026, 12, 21), date(2026, 12, 27)),
        Some(true)
    );
    assert_eq!(
        holidays.touches(date(2026, 12, 14), date(2026, 12, 20)),
        Some(false)
    );
}

/// **A week touched at either end counts.** The week before half term that
/// runs into its Monday, and a week whose Friday is half term's last day.
#[test]
fn a_week_that_only_overlaps_the_edge_is_touched() {
    let holidays = christmas().expect("the holidays build");

    assert_eq!(
        holidays.touches(date(2026, 10, 19), date(2026, 10, 26)),
        Some(true)
    );
    assert_eq!(
        holidays.touches(date(2026, 10, 30), date(2026, 11, 5)),
        Some(true)
    );
    assert_eq!(
        holidays.touches(date(2026, 10, 31), date(2026, 11, 6)),
        Some(false)
    );
}

/// **Past the school's calendar there is no data, not no holiday.** The
/// operator, 2026-09-22: *"That shouldn't be read as no holiday, it should be
/// read as no data."* gov.uk still speaks for the week; the school does not.
#[test]
fn past_the_schools_calendar_there_is_no_data() {
    let holidays = christmas().expect("the holidays build");

    assert_eq!(
        holidays.touches(date(2027, 1, 4), date(2027, 1, 10)),
        None,
        "the school has published nothing for this week"
    );
}

/// **A known holiday is known whatever else is not.** Boxing Day is a public
/// holiday, so the week touches one, even with the school silent on it.
#[test]
fn a_known_holiday_answers_even_where_the_other_source_is_silent() {
    let Some(one) = NonZeroU8::new(1) else {
        panic!("one is not zero")
    };
    let school = Holidays::from_school(
        vec![SchoolHoliday::new(date(2026, 10, 26), one)],
        Some(date(2026, 12, 18)),
    );
    let public = Holidays::from_public(
        vec![PublicHoliday::new(
            date(2026, 12, 28),
            "Boxing Day".to_owned(),
        )],
        Some(date(2028, 12, 26)),
    );

    assert_eq!(
        school
            .and(public)
            .touches(date(2026, 12, 28), date(2027, 1, 3)),
        Some(true)
    );
}

/// A public holiday in term time is enough.
#[test]
fn a_public_holiday_in_term_touches_its_week() {
    let school = Holidays::from_school(Vec::new(), Some(date(2027, 9, 5)));
    let public = Holidays::from_public(
        vec![PublicHoliday::new(
            date(2027, 5, 3),
            "Early May bank holiday".to_owned(),
        )],
        Some(date(2028, 12, 26)),
    );
    let holidays = school.and(public);

    assert_eq!(
        holidays.touches(date(2027, 5, 3), date(2027, 5, 9)),
        Some(true)
    );
    assert_eq!(
        holidays.touches(date(2027, 5, 4), date(2027, 5, 9)),
        Some(false)
    );
}

/// The last day of a fourteen-day run from the 21st is the 3rd, and the
/// school is back on the 4th.
#[test]
fn a_school_holiday_ends_on_its_last_day() {
    let holidays = christmas().expect("the holidays build");
    let Some(christmas) = holidays.school().last() else {
        panic!("two school holidays")
    };
    assert_eq!(christmas.last(), date(2027, 1, 3));
}

/// Only what has not ended, in the order it falls, and the reach kept.
#[test]
fn holidays_since_a_date_drop_what_has_ended_and_keep_the_reach() {
    let holidays = christmas()
        .expect("the holidays build")
        .since(date(2026, 11, 1));

    assert_eq!(holidays.school().len(), 1, "half term has ended");
    let names: Vec<&str> = holidays.public().iter().map(PublicHoliday::name).collect();
    assert_eq!(names, ["Christmas Day", "Boxing Day"]);
    assert_eq!(holidays.school_published_to(), Some(date(2027, 1, 3)));
}
