//! The macrocycle, on the school's real calendar for 2026–27 (#224).

use std::num::NonZeroU8;

use domain::{
    macrocycle::{Chain, Concurrent, Cycling, Macrocycle, Phase, Term},
    plan::Span,
    schedule::{Holidays, PublicHoliday, SchoolHoliday, SchoolHolidayKind},
};
use jiff::civil::{Date, date};

/// A school holiday from its first and last days. `None` for a run the type
/// cannot hold, which none of these is.
fn holiday(first: Date, last: Date) -> Option<SchoolHoliday> {
    let days = (last - first).get_days().checked_add(1)?;
    Some(SchoolHoliday::new(
        first,
        NonZeroU8::new(u8::try_from(days).ok()?)?,
    ))
}

/// Every school holiday from Summer 2026 to Summer 2027 as the school's feed
/// published them on 2026-09-24, and the public holidays that name them as gov.uk does.
fn calendar() -> Option<Holidays> {
    let school = Holidays::from_school(
        vec![
            holiday(date(2026, 7, 21), date(2026, 9, 5))?,
            holiday(date(2026, 10, 26), date(2026, 10, 30))?,
            holiday(date(2026, 12, 21), date(2027, 1, 3))?,
            holiday(date(2027, 2, 8), date(2027, 2, 12))?,
            holiday(date(2027, 3, 22), date(2027, 4, 2))?,
            holiday(date(2027, 5, 31), date(2027, 6, 4))?,
            holiday(date(2027, 7, 27), date(2027, 9, 5))?,
        ],
        Some(date(2027, 9, 5)),
    );
    let public = Holidays::from_public(
        vec![
            PublicHoliday::new(date(2026, 8, 31), "Summer bank holiday".to_owned())
                .naming(SchoolHolidayKind::Summer),
            PublicHoliday::new(date(2026, 12, 25), "Christmas Day".to_owned()),
            PublicHoliday::new(date(2027, 3, 26), "Good Friday".to_owned())
                .naming(SchoolHolidayKind::Easter),
            PublicHoliday::new(date(2027, 5, 31), "Spring bank holiday".to_owned()),
            PublicHoliday::new(date(2027, 8, 30), "Summer bank holiday".to_owned())
                .naming(SchoolHolidayKind::Summer),
        ],
        Some(date(2027, 12, 31)),
    );
    Some(school.and(public))
}

/// **Today is in autumn 2026**, which starts on the Monday of the first week
/// of term and whose last week of term is the week of 14 December.
#[test]
fn today_is_in_autumn_2026() {
    let holidays = calendar().expect("the calendar builds");
    let autumn = Macrocycle::on(date(2026, 9, 24), &holidays).expect("a macrocycle");

    assert_eq!(autumn.term(), Term::Autumn);
    assert_eq!(autumn.to_string(), "autumn 2026");
    assert_eq!(autumn.start(), date(2026, 9, 7));
    assert_eq!(autumn.last_day_of_term(), date(2026, 12, 20));
    assert_eq!(autumn.transition(), date(2026, 12, 21));
}

/// **A half term is inside a term, not the end of one.**
#[test]
fn half_term_is_inside_autumn() {
    let holidays = calendar().expect("the calendar builds");
    let during = Macrocycle::on(date(2026, 10, 28), &holidays).expect("a macrocycle");

    assert_eq!(during.to_string(), "autumn 2026");
    assert_eq!(during.phase(date(2026, 10, 28), None), Phase::Preparatory);
}

/// **Christmas is autumn's transition**, and spring starts the Monday after
/// it.
#[test]
fn christmas_is_autumns_transition_and_spring_follows_it() {
    let holidays = calendar().expect("the calendar builds");

    let christmas = Macrocycle::on(date(2027, 1, 1), &holidays).expect("a macrocycle");
    assert_eq!(christmas.to_string(), "autumn 2026");
    assert_eq!(christmas.last(), date(2027, 1, 3));
    assert_eq!(christmas.phase(date(2027, 1, 1), None), Phase::Transition);

    let spring = Macrocycle::on(date(2027, 1, 4), &holidays).expect("a macrocycle");
    assert_eq!(spring.start(), date(2027, 1, 4));
    assert_eq!(spring.term(), Term::Spring);
    assert_eq!(spring.to_string(), "spring 2027");
    assert_eq!(spring.last_day_of_term(), date(2027, 3, 21));
}

/// **A Summer holiday starting on a Tuesday takes that whole week**, so the
/// summer term's last week is the one before.
#[test]
fn summer_2027_takes_the_week_it_starts_in() {
    let holidays = calendar().expect("the calendar builds");
    let summer = Macrocycle::on(date(2027, 6, 1), &holidays).expect("a macrocycle");

    assert_eq!(summer.to_string(), "summer 2027");
    assert_eq!(summer.start(), date(2027, 4, 5));
    assert_eq!(summer.last_day_of_term(), date(2027, 7, 25));
}

/// **Competitive only for the mesocycle ending on the last day of term.** The
/// store's last autumn mesocycle, 16 November to 13 December, stops a week
/// short and so is preparatory. The one the operator would have run in
/// hindsight, 23 November to 20 December, is competitive.
#[test]
fn only_a_mesocycle_ending_with_the_term_is_competitive() {
    let holidays = calendar().expect("the calendar builds");
    let autumn = Macrocycle::on(date(2026, 12, 1), &holidays).expect("a macrocycle");

    let stored = Span::new(date(2026, 11, 16), 4);
    assert_eq!(
        autumn.phase(date(2026, 12, 1), Some(stored)),
        Phase::Preparatory
    );

    let hindsight = Span::new(date(2026, 11, 23), 4);
    assert_eq!(
        autumn.phase(date(2026, 12, 1), Some(hindsight)),
        Phase::Competitive
    );
}

/// **Past the calendar's reach there is no macrocycle**: no data, not no
/// term. And before the first holiday it knows ends a term, there is no
/// start to count from.
#[test]
fn outside_the_calendar_there_is_no_macrocycle() {
    let holidays = calendar().expect("the calendar builds");

    assert_eq!(Macrocycle::on(date(2027, 10, 1), &holidays), None);
    assert_eq!(Macrocycle::on(date(2026, 7, 1), &holidays), None);
}

fn autumn_2026() -> Option<Macrocycle> {
    Macrocycle::on(date(2026, 9, 24), &calendar()?)
}

/// **Scenario 1 of #222: the entry tests are performed w/c 21 September.**
/// The term runs SBS + Build, SBS + Peak 1, SBS + Peak 2, and the last test
/// week is w/c 14 December.
#[test]
fn tests_on_21_september_run_build_then_peak_to_the_end_of_term() {
    let autumn = autumn_2026().expect("autumn 2026");
    let chain = Chain::BuildThenPeak;
    let tested = Some(Cycling::Hold { holding: 0 });

    let first = autumn.next(chain, tested, date(2026, 9, 28));
    assert_eq!(first, Some(Concurrent::progressing(Cycling::Build)));

    let second = autumn.next(chain, Some(Cycling::Build), date(2026, 10, 26));
    assert_eq!(second, Some(Concurrent::progressing(Cycling::Peak1)));

    let third = autumn.next(chain, Some(Cycling::Peak1), date(2026, 11, 23));
    assert_eq!(third, Some(Concurrent::progressing(Cycling::Peak2)));
}

/// **Scenario 2 of #222: the entry tests slip to w/c 28 September.** Peak 1
/// would fit on 2 November, but nothing ending in tests fits after it, so both
/// disciplines hold for two weeks and test w/c 16 November. Build opens from
/// those tests and ends the term.
#[test]
fn tests_on_28_september_hold_in_november_and_build_again() {
    let autumn = autumn_2026().expect("autumn 2026");
    let chain = Chain::BuildThenPeak;

    let first = autumn.next(chain, Some(Cycling::Hold { holding: 0 }), date(2026, 10, 5));
    assert_eq!(first, Some(Concurrent::progressing(Cycling::Build)));

    let held = autumn.next(chain, Some(Cycling::Build), date(2026, 11, 2));
    assert_eq!(held, Some(Concurrent::hold(2)));

    let last = autumn.next(
        chain,
        Some(Cycling::Hold { holding: 2 }),
        date(2026, 11, 23),
    );
    assert_eq!(last, Some(Concurrent::progressing(Cycling::Build)));
}

/// **A term starts from the entry tests**: nothing carries across the
/// holiday, so the first thing committed is a hold that ends in them, as
/// late as still leaves the chain room.
#[test]
fn a_term_opens_with_a_hold_that_ends_in_the_entry_tests() {
    let first =
        autumn_2026()
            .expect("autumn 2026")
            .next(Chain::BuildThenPeak, None, date(2026, 9, 7));
    assert_eq!(
        first,
        Some(Concurrent::hold(2)),
        "two holding weeks and the tests w/c 21 September leave exactly twelve"
    );
}

#[test]
fn base_then_build_runs_base_one_base_two_and_build() {
    let autumn = autumn_2026().expect("autumn 2026");
    let chain = Chain::BaseThenBuild;

    let first = autumn.next(chain, Some(Cycling::Hold { holding: 0 }), date(2026, 9, 28));
    assert_eq!(first, Some(Concurrent::progressing(Cycling::Base1)));
    let second = autumn.next(chain, Some(Cycling::Base1), date(2026, 10, 26));
    assert_eq!(second, Some(Concurrent::progressing(Cycling::Base2)));
    let third = autumn.next(chain, Some(Cycling::Base2), date(2026, 11, 23));
    assert_eq!(third, Some(Concurrent::progressing(Cycling::Build)));
}

/// **Base 1 → hold → Build** when a lost week leaves no room for Base 2 and
/// Build after it. Base 1 has no exit test, so Build cannot follow it directly.
#[test]
fn a_disrupted_base_holds_and_then_builds() {
    let held = autumn_2026().expect("autumn 2026").next(
        Chain::BaseThenBuild,
        Some(Cycling::Base1),
        date(2026, 11, 2),
    );
    assert_eq!(held, Some(Concurrent::hold(2)));
}

/// **Never across the holiday**, and never nothing while term remains.
#[test]
fn nothing_is_committed_that_runs_into_christmas() {
    let autumn = autumn_2026().expect("autumn 2026");
    let chain = Chain::BuildThenPeak;

    let last_week = autumn.next(chain, Some(Cycling::Build), date(2026, 12, 14));
    assert_eq!(
        last_week,
        Some(Concurrent::hold(0)),
        "one week is a test week"
    );

    let three = autumn.next(chain, Some(Cycling::Build), date(2026, 11, 30));
    assert_eq!(
        three,
        Some(Concurrent::hold(2)),
        "a hold to the end of term"
    );

    assert_eq!(
        autumn.next(chain, Some(Cycling::Build), date(2026, 12, 21)),
        None
    );
}

#[test]
fn every_mesocycle_committed_through_the_term_ends_by_the_transition() {
    let autumn = autumn_2026().expect("autumn 2026");
    for chain in [Chain::BaseThenBuild, Chain::BuildThenPeak] {
        for weeks_in in 0..15_i64 {
            let start = date(2026, 9, 7)
                .checked_add(jiff::Span::new().weeks(weeks_in))
                .expect("a date in term");
            for previous in [
                None,
                Some(Cycling::Hold { holding: 0 }),
                Some(Cycling::Base1),
                Some(Cycling::Build),
                Some(Cycling::Peak1),
            ] {
                let Some(next) = autumn.next(chain, previous, start) else {
                    continue;
                };
                let end = start
                    .checked_add(jiff::Span::new().weeks(i64::from(next.weeks())))
                    .expect("a date");
                assert!(
                    end <= autumn.transition(),
                    "{next} from {start} after {previous:?} runs into Christmas"
                );
                assert!(
                    next.cycling().may_follow(previous),
                    "{next} may not follow {previous:?}"
                );
            }
        }
    }
}
