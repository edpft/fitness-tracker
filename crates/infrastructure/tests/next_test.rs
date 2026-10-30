//! The next planned test, read off the plan's calendar (#420).
//!
//! Here rather than in `domain` because the plan fixtures are here; the
//! function is `domain`'s and touches no adapter.

use crate::support;

use domain::analytical::one_rep_max::{NextTest, next_test};
use jiff::civil::date;
use support::mesocycle::{entry_test_from, plan};

#[test]
fn a_test_week_tests_on_its_last_programmed_day() {
    // Monday light, Friday heavy, in the week of Monday 2026-09-14.
    let plan = plan(vec![
        entry_test_from(date(2026, 9, 14)).expect("a test week"),
    ])
    .expect("a plan");
    assert_eq!(
        next_test(&[plan], date(2026, 9, 13)),
        Some(NextTest {
            on: date(2026, 9, 18),
            sessions: 2,
        })
    );
}

#[test]
fn nothing_planned_after_the_test_is_no_next_test() {
    let plan = plan(vec![
        entry_test_from(date(2026, 9, 14)).expect("a test week"),
    ])
    .expect("a plan");
    assert_eq!(next_test(&[plan], date(2026, 9, 18)), None);
}
