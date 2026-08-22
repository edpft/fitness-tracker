//! What makes two programmes rivals, and what makes them one programme.

use domain::prescription::{InvalidProgrammeName, ProgrammeName, ProgrammeWindow};
use jiff::civil::Date;

fn name(text: &str) -> Result<ProgrammeName, InvalidProgrammeName> {
    ProgrammeName::try_from(text.to_owned())
}

fn window(called: &str, start: Date, weeks: u32) -> Result<ProgrammeWindow, InvalidProgrammeName> {
    Ok(ProgrammeWindow::new(name(called)?, start, weeks))
}

/// Surrounding whitespace is trimmed rather than refused.
///
/// **Because it is an identity.** `"autumn"` and `" autumn"` must not be two
/// programmes, and a document a person edits will sooner or later carry one.
#[test]
fn a_name_is_trimmed_so_one_label_is_one_programme() {
    let (Ok(bare), Ok(padded)) = (name("autumn"), name("  autumn  ")) else {
        panic!("both are usable names")
    };
    assert_eq!(bare, padded);
}

#[test]
fn a_name_must_be_one_printable_line() {
    assert_eq!(name(""), Err(InvalidProgrammeName::Empty));
    assert_eq!(name("   "), Err(InvalidProgrammeName::Empty));
    assert_eq!(
        name("autumn\nblock"),
        Err(InvalidProgrammeName::NotPrintable)
    );
    let long = "a".repeat(65);
    assert_eq!(
        name(&long),
        Err(InvalidProgrammeName::TooLong { length: 65 })
    );
}

/// Versions of one programme never compete, however much they overlap.
///
/// The whole point of the name: re-authoring a block to correct it must not be
/// refused for overlapping the block it corrects.
#[test]
fn one_name_is_one_programme_however_it_is_re_authored() {
    let (Ok(first), Ok(corrected)) = (
        window("summer", Date::constant(2026, 8, 3), 6),
        window("summer", Date::constant(2026, 8, 3), 5),
    ) else {
        panic!("the windows are valid")
    };
    assert!(!first.overlaps(&corrected));
    assert!(!corrected.overlaps(&first));
}

/// A programme starting the day after another ends is adjacent, not overlapping.
///
/// The common case, and the one an inclusive end date would refuse: the summer
/// block's last day is Sunday and the autumn block opens on the Monday.
#[test]
fn adjacent_programmes_do_not_overlap() {
    let (Ok(summer), Ok(autumn)) = (
        // Five weeks from Monday 3 August ends after Sunday 6 September.
        window("summer", Date::constant(2026, 8, 3), 5),
        window("autumn", Date::constant(2026, 9, 7), 8),
    ) else {
        panic!("the windows are valid")
    };
    assert!(!summer.overlaps(&autumn));
    assert!(!autumn.overlaps(&summer));
}

#[test]
fn differently_named_programmes_sharing_a_day_overlap() {
    let (Ok(summer), Ok(autumn)) = (
        window("summer", Date::constant(2026, 8, 3), 5),
        // One week early: it claims the summer block's last week.
        window("autumn", Date::constant(2026, 8, 31), 8),
    ) else {
        panic!("the windows are valid")
    };
    assert!(summer.overlaps(&autumn), "and the rule is symmetric");
    assert!(autumn.overlaps(&summer));
}

/// A window covers its first day and not the day it ends on.
#[test]
fn a_window_is_half_open() {
    let Ok(summer) = window("summer", Date::constant(2026, 8, 3), 5) else {
        panic!("the window is valid")
    };
    assert!(summer.covers(Date::constant(2026, 8, 3)), "its first day");
    assert!(summer.covers(Date::constant(2026, 9, 6)), "its last day");
    assert!(
        !summer.covers(Date::constant(2026, 9, 7)),
        "and not the day the next block opens"
    );
    assert!(!summer.covers(Date::constant(2026, 8, 2)));
}
