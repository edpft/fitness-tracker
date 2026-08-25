//! When there is room to train, through its real store (§ III).
//!
//! Authored data like the programme beside it: nothing derives a pattern from
//! the record, because the record shows when the operator *did* train and that
//! is not the same as when they could have.

mod support;

use std::{collections::BTreeSet, num::NonZeroU8};

use application::{DiaryAuthor as _, DiaryStore as _};
use domain::{
    gym::OperatorZone,
    schedule::{Alteration, PartOfDay, TrainingPattern, TrainingSlot},
};
use infrastructure::{SqliteDiaryStore, connect};
use jiff::civil::{Weekday, date};
use support::corpus;

async fn store() -> Result<(SqliteDiaryStore, tempfile::TempDir), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    Ok((SqliteDiaryStore::new(pool), directory))
}

macro_rules! opened {
    () => {
        match corpus::block_on(store()) {
            Ok(Ok(opened)) => opened,
            Ok(Err(error)) => panic!("a store opens: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

macro_rules! run {
    ($body:expr) => {
        match corpus::block_on($body) {
            Ok(Ok(value)) => value,
            Ok(Err(error)) => panic!("the store answers: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// A macro rather than a function, because `panic` is forbidden in a free
/// function even in a test file — the exemption reaches `#[test]` bodies, and a
/// macro expands into one.
macro_rules! zone {
    ($name:literal) => {
        match OperatorZone::try_from($name.to_owned()) {
            Ok(zone) => zone,
            Err(error) => panic!("{} is a zone: {error}", $name),
        }
    };
}

macro_rules! days {
    ($count:literal) => {
        match NonZeroU8::new($count) {
            Some(days) => days,
            None => panic!("{} is not zero", $count),
        }
    };
}

fn slots(of: &[(Weekday, PartOfDay)]) -> BTreeSet<TrainingSlot> {
    of.iter()
        .map(|(day, part)| TrainingSlot::new(*day, *part))
        .collect()
}

/// The operator's ordinary pattern, as stated on 2026-08-24.
fn ordinary_pattern() -> BTreeSet<TrainingSlot> {
    slots(&[
        (Weekday::Monday, PartOfDay::Evening),
        (Weekday::Wednesday, PartOfDay::Evening),
        (Weekday::Friday, PartOfDay::Evening),
        (Weekday::Sunday, PartOfDay::Morning),
    ])
}

/// A pattern and an alteration go in and come back the same.
#[test]
fn a_pattern_and_its_alterations_round_trip() {
    let (store, _directory) = opened!();

    let pattern = TrainingPattern::new(
        date(2026, 8, 24),
        zone!("Europe/London"),
        ordinary_pattern(),
    );
    let alteration = Alteration::new(
        date(2026, 9, 14),
        days!(1),
        None,
        Some(BTreeSet::new()),
        "away, and unable to train".to_owned(),
    );

    run!(store.record_pattern(&pattern));
    run!(store.record_alteration(&alteration));

    let diary = run!(store.diary());

    assert_eq!(diary.patterns(), [pattern], "the pattern reads back");
    assert_eq!(
        diary.alterations(),
        [alteration],
        "the alteration reads back"
    );
}

/// **The distinction the schema exists to keep.**
///
/// An alteration's slots are `Option<BTreeSet<TrainingSlot>>`: absent means the ordinary week
/// stands — away, training as usual — and present-but-empty means no room to
/// train at all. Both are zero rows in `schedule_patch_slot`, so storage has to
/// carry the difference some other way, and collapsing them would make a
/// zone-only alteration silently cancel every session of a trip.
#[test]
fn a_patch_that_states_no_slots_is_not_a_patch_that_states_none() {
    let (store, _directory) = opened!();

    run!(store.record_pattern(&TrainingPattern::new(
        date(2026, 8, 24),
        zone!("Europe/London"),
        ordinary_pattern()
    )));

    // Away, training as usual, in another country.
    let unchanged = Alteration::new(
        date(2026, 10, 5),
        days!(1),
        Some(zone!("Europe/Rome")),
        None,
        "in Rome, training as usual".to_owned(),
    );
    // Away, and unable to train.
    let cancelled = Alteration::new(
        date(2026, 9, 14),
        days!(1),
        None,
        Some(BTreeSet::new()),
        "away, and unable to train".to_owned(),
    );

    run!(store.record_alteration(&unchanged));
    run!(store.record_alteration(&cancelled));

    let diary = run!(store.diary());

    let Some(rome) = diary
        .alterations()
        .iter()
        .find(|p| p.start() == date(2026, 10, 5))
    else {
        panic!("the Rome alteration is stored")
    };
    let Some(away) = diary
        .alterations()
        .iter()
        .find(|p| p.start() == date(2026, 9, 14))
    else {
        panic!("the 14 September alteration is stored")
    };

    assert_eq!(
        rome.slots(),
        None,
        "a zone-only alteration changes no slots"
    );
    assert_eq!(
        away.slots(),
        Some(&BTreeSet::new()),
        "an unavailable day states the empty set"
    );

    // And the consequence, which is the reason the distinction is kept: the
    // Monday in Rome is still a training day, and 14 September is not.
    let Some(in_rome) = diary.on(date(2026, 10, 5)) else {
        panic!("the diary answers a date it covers")
    };
    assert_eq!(in_rome.zone.id(), "Europe/Rome");
    assert!(in_rome.open(date(2026, 10, 5)), "a Monday in Rome is open");

    let Some(on_the_14th) = diary.on(date(2026, 9, 14)) else {
        panic!("the diary answers a date it covers")
    };
    assert!(
        !on_the_14th.open(date(2026, 9, 14)),
        "14 September is not a day the operator can train"
    );
}

/// **What step 2 will ask, answered from the store.**
///
/// The goal is a test week commencing Monday 14 September, and the whole of
/// what it needs from the schedule is that the 14th is gone. A programme takes
/// the slots it has been allocated and asks which of its days it loses.
#[test]
fn the_fourteenth_of_september_is_the_day_the_programme_loses() {
    let (store, _directory) = opened!();

    run!(store.record_pattern(&TrainingPattern::new(
        date(2026, 8, 24),
        zone!("Europe/London"),
        ordinary_pattern()
    )));
    run!(store.record_alteration(&Alteration::new(
        date(2026, 9, 14),
        days!(1),
        None,
        Some(BTreeSet::new()),
        "away, and unable to train".to_owned(),
    )));

    let diary = run!(store.diary());
    let allocated = ordinary_pattern();

    let lost = diary.unavailable(date(2026, 9, 14), date(2026, 9, 20), &allocated);

    assert_eq!(
        lost,
        [date(2026, 9, 14)],
        "the week of the 14th loses the Monday and nothing else"
    );
}

/// Re-stating the pattern in force from a date corrects it rather than adding a
/// second one that begins the same day.
///
/// Succession is a *later* date. Two rows sharing one start could not be
/// ordered, and `Diary::on` takes the last that applies.
#[test]
fn re_stating_a_pattern_corrects_it() {
    let (store, _directory) = opened!();

    run!(store.record_pattern(&TrainingPattern::new(
        date(2026, 8, 24),
        zone!("Europe/London"),
        ordinary_pattern()
    )));
    run!(store.record_pattern(&TrainingPattern::new(
        date(2026, 8, 24),
        zone!("Europe/London"),
        slots(&[(Weekday::Tuesday, PartOfDay::Morning)]),
    )));

    let diary = run!(store.diary());

    assert_eq!(diary.patterns().len(), 1, "one pattern, corrected");
    assert_eq!(
        diary.patterns()[0].slots(),
        &slots(&[(Weekday::Tuesday, PartOfDay::Morning)]),
        "the correction is what stands"
    );
}

/// A later pattern supersedes an earlier one by existing, and both are kept.
#[test]
fn a_later_pattern_supersedes_by_existing() {
    let (store, _directory) = opened!();

    run!(store.record_pattern(&TrainingPattern::new(
        date(2026, 8, 24),
        zone!("Europe/London"),
        ordinary_pattern()
    )));
    run!(store.record_pattern(&TrainingPattern::new(
        date(2026, 9, 21),
        zone!("Europe/London"),
        slots(&[(Weekday::Saturday, PartOfDay::Morning)]),
    )));

    let diary = run!(store.diary());

    assert_eq!(diary.patterns().len(), 2, "both patterns are kept");

    let Some(before) = diary.on(date(2026, 9, 1)) else {
        panic!("a date the diary covers")
    };
    let Some(after) = diary.on(date(2026, 9, 28)) else {
        panic!("a date the diary covers")
    };

    assert_eq!(
        before.slots,
        ordinary_pattern(),
        "the first pattern still answers"
    );
    assert_eq!(
        after.slots,
        slots(&[(Weekday::Saturday, PartOfDay::Morning)]),
        "the later pattern answers from its own date"
    );

    // Before the first schedule, the operator has said nothing — which is not
    // the same as having said "no slots".
    assert!(
        diary.on(date(2026, 8, 1)).is_none(),
        "a date before the first pattern is unknown, not empty"
    );
}
