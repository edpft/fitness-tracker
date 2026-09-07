//! What a plan holds, what it occupies, and what may not compete with it.

use domain::plan::{
    EmptyPlan, InvalidPlanName, InvalidProgramme, Occupies, Plan, PlanName, PlanWindow, Programme,
    Span,
};
use jiff::civil::Date;

/// Something that occupies days and is nothing else.
///
/// **A stand-in rather than a real mesocycle**, because what is under test is
/// the container: `Programme` is generic over its mesocycle precisely so that
/// ordering, overlap and answering for a date are written once, and a fixture
/// carrying a lift or a zone plan would test the fixture too.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Occupant(Span);

impl Occupies for Occupant {
    fn span(&self) -> Span {
        self.0
    }
}

const fn at(year: i16, month: i8, day: i8, weeks: u32) -> Occupant {
    Occupant(Span::new(Date::constant(year, month, day), weeks))
}

fn name(text: &str) -> Result<PlanName, InvalidPlanName> {
    PlanName::try_from(text.to_owned())
}

fn window(called: &str, start: Date, weeks: u32) -> Result<PlanWindow, InvalidPlanName> {
    Ok(PlanWindow::new(name(called)?, Span::new(start, weeks)))
}

// --- the span ---------------------------------------------------------------

/// A span covers its first day and not the day it ends on.
#[test]
fn a_span_is_half_open() {
    let summer = Span::new(Date::constant(2026, 8, 3), 5);
    assert!(summer.covers(Date::constant(2026, 8, 3)), "its first day");
    assert!(summer.covers(Date::constant(2026, 9, 6)), "its last day");
    assert!(
        !summer.covers(Date::constant(2026, 9, 7)),
        "and not the day the next block opens"
    );
    assert!(!summer.covers(Date::constant(2026, 8, 2)));
}

/// A mesocycle starting the Monday after another ends is adjacent, not
/// overlapping. The common case, and the one an inclusive end date would refuse.
#[test]
fn adjacent_spans_do_not_overlap() {
    let summer = Span::new(Date::constant(2026, 8, 3), 5);
    let autumn = Span::new(Date::constant(2026, 9, 7), 8);
    assert!(!summer.overlaps(autumn));
    assert!(!autumn.overlaps(summer));
}

/// Joining fills the gap between two spans, because a plan occupies the weeks
/// it is not training in as much as the weeks it is.
#[test]
fn joining_covers_the_gap_between() {
    let first = Span::new(Date::constant(2026, 9, 14), 1);
    let last = Span::new(Date::constant(2026, 10, 19), 8);
    let joined = first.joined(last);
    assert_eq!(joined.start(), Date::constant(2026, 9, 14));
    assert_eq!(joined.calendar_weeks(), 13);
    assert!(joined.covers(Date::constant(2026, 9, 28)), "the blank week");
}

// --- the plan's window ------------------------------------------------------

/// Versions of one plan never compete, however much they overlap.
///
/// The whole point of the name: re-authoring the autumn to correct it must not
/// be refused for overlapping the autumn it corrects.
#[test]
fn one_name_is_one_plan_however_it_is_re_authored() {
    let (Ok(first), Ok(corrected)) = (
        window("autumn", Date::constant(2026, 9, 14), 13),
        window("autumn", Date::constant(2026, 9, 14), 12),
    ) else {
        panic!("the windows are valid")
    };
    assert!(!first.overlaps(&corrected));
    assert!(!corrected.overlaps(&first));
}

#[test]
fn differently_named_plans_sharing_a_day_overlap() {
    let (Ok(summer), Ok(autumn)) = (
        window("summer", Date::constant(2026, 8, 3), 5),
        // One week early: it claims the summer plan's last week.
        window("autumn", Date::constant(2026, 8, 31), 8),
    ) else {
        panic!("the windows are valid")
    };
    assert!(summer.overlaps(&autumn), "and the rule is symmetric");
    assert!(autumn.overlaps(&summer));
}

/// A name is trimmed rather than refused, so one label is one plan.
#[test]
fn a_plan_name_is_one_printable_trimmed_line() {
    let (Ok(bare), Ok(padded)) = (name("autumn"), name("  autumn  ")) else {
        panic!("both are usable names")
    };
    assert_eq!(bare, padded);
    assert_eq!(name(""), Err(InvalidPlanName::Empty));
    assert_eq!(name("autumn\nplan"), Err(InvalidPlanName::NotPrintable));
    assert_eq!(
        name(&"a".repeat(65)),
        Err(InvalidPlanName::TooLong { length: 65 })
    );
}

// --- the programme ----------------------------------------------------------

/// The autumn's gym side: an entry test, then three cycles of four.
fn autumn_gym() -> Vec<Occupant> {
    vec![
        at(2026, 9, 14, 1),
        at(2026, 9, 21, 4),
        at(2026, 10, 19, 4),
        at(2026, 11, 16, 4),
    ]
}

#[test]
fn a_programme_spans_its_first_mesocycle_to_its_last() {
    let Ok(gym) = Programme::new(autumn_gym()) else {
        panic!("the autumn is a valid programme")
    };
    assert_eq!(gym.count(), 4);
    assert_eq!(gym.span().start(), Date::constant(2026, 9, 14));
    assert_eq!(gym.span().calendar_weeks(), 13);
}

#[test]
fn a_programme_answers_for_a_date_with_where_it_sits() {
    let Ok(gym) = Programme::new(autumn_gym()) else {
        panic!("the autumn is a valid programme")
    };
    let Some((at, _)) = gym.on(Date::constant(2026, 10, 20)) else {
        panic!("the second cycle covers it")
    };
    assert_eq!(at, 2, "counting from zero: the third mesocycle");
    assert!(
        gym.on(Date::constant(2026, 9, 13)).is_none(),
        "the day before"
    );
    assert!(
        gym.on(Date::constant(2026, 12, 14)).is_none(),
        "the day after"
    );
}

/// A blank week between two mesocycles belongs to neither. A real thing — a
/// fortnight away, a deload nobody programmed — and not a fault.
#[test]
fn a_gap_belongs_to_no_mesocycle() {
    let Ok(interrupted) = Programme::new(vec![at(2026, 9, 14, 1), at(2026, 9, 28, 4)]) else {
        panic!("a gap is legal")
    };
    assert!(interrupted.on(Date::constant(2026, 9, 22)).is_none());
    assert_eq!(interrupted.span().calendar_weeks(), 6, "the gap included");
}

/// Two mesocycles competing for one day would make the answer to "what am I
/// doing today" depend on the order rows came back in.
#[test]
fn two_mesocycles_of_one_programme_may_not_compete() {
    assert_eq!(
        Programme::new(vec![at(2026, 9, 14, 4), at(2026, 10, 5, 4)]),
        Err(InvalidProgramme::OutOfOrder {
            at: 2,
            before: 1,
            start: Date::constant(2026, 10, 5),
        })
    );
    assert_eq!(
        Programme::new(Vec::<Occupant>::new()),
        Err(InvalidProgramme::NoMesocycles)
    );
}

/// What a test takes its target from is the previous element of a list, not a
/// question for the store — except for the mesocycle that opens a plan.
#[test]
fn the_preceding_mesocycle_is_the_last_one_to_have_finished() {
    let Ok(gym) = Programme::new(autumn_gym()) else {
        panic!("the autumn is a valid programme")
    };
    assert_eq!(
        gym.preceding(Date::constant(2026, 10, 19)),
        Some(&at(2026, 9, 21, 4)),
        "a cycle still running is not it"
    );
    assert!(
        gym.preceding(Date::constant(2026, 9, 14)).is_none(),
        "nothing precedes the mesocycle that opens the plan"
    );
}

// --- the plan ---------------------------------------------------------------

/// A plan trains something.
#[test]
fn a_plan_holding_neither_discipline_is_refused() {
    let Ok(autumn) = name("autumn") else {
        panic!("the name is valid")
    };
    assert_eq!(
        Plan::new(autumn, jiff::Timestamp::UNIX_EPOCH, None, None),
        Err(EmptyPlan)
    );
}
