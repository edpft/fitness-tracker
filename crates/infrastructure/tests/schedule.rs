//! When there is room to train, through its real store (§ III).
//!
//! Authored data like the programme beside it: nothing derives a pattern from
//! the record, because the record shows when the operator *did* train and that
//! is not the same as when they could have.

mod support;

use std::{collections::BTreeMap, num::NonZeroU8};

use application::{DiaryAuthor as _, DiaryStore as _};
use domain::{
    normalised::OperatorZone,
    schedule::{Absence, Alteration, Discipline, PartOfDay, TrainingPattern, TrainingSlot},
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

fn slots(of: &[(Weekday, PartOfDay, Discipline)]) -> BTreeMap<TrainingSlot, Discipline> {
    of.iter()
        .map(|(day, part, discipline)| (TrainingSlot::new(*day, *part), *discipline))
        .collect()
}

/// The operator's ordinary pattern, as stated on 2026-08-24.
fn ordinary_pattern() -> BTreeMap<TrainingSlot, Discipline> {
    slots(&[
        (Weekday::Monday, PartOfDay::Evening, Discipline::Gym),
        (Weekday::Wednesday, PartOfDay::Evening, Discipline::Cycling),
        (Weekday::Friday, PartOfDay::Evening, Discipline::Gym),
        (Weekday::Sunday, PartOfDay::Morning, Discipline::Cycling),
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
        Absence::Holiday {
            zone: None,
            slots: BTreeMap::new(),
        },
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
/// A holiday and an illness come back as what they were recorded as.
///
/// Both leave no room to train and both are zero rows in `alteration_slot`, so
/// the kind is the only thing telling them apart — and an illness read back as
/// a holiday would be a week of lost fitness read as a week of rest.
#[test]
fn an_illness_is_not_a_holiday() {
    let (store, _directory) = opened!();

    run!(store.record_pattern(&TrainingPattern::new(
        date(2026, 8, 24),
        zone!("Europe/London"),
        ordinary_pattern()
    )));

    let holiday = Alteration::new(
        date(2026, 10, 5),
        days!(3),
        Absence::Holiday {
            zone: Some(zone!("Europe/Rome")),
            slots: BTreeMap::new(),
        },
        "Rome".to_owned(),
    );
    let illness = Alteration::new(
        date(2026, 9, 19),
        days!(2),
        Absence::Illness,
        "a cold".to_owned(),
    );

    run!(store.record_alteration(&holiday));
    run!(store.record_alteration(&illness));

    let diary = run!(store.diary());

    assert_eq!(
        diary
            .alterations()
            .iter()
            .find(|a| a.start() == date(2026, 10, 5)),
        Some(&holiday),
        "the holiday is stored with its zone"
    );
    assert_eq!(
        diary
            .alterations()
            .iter()
            .find(|a| a.start() == date(2026, 9, 19)),
        Some(&illness),
        "the illness is stored as illness"
    );

    let Some(ill) = diary.on(date(2026, 9, 20)) else {
        panic!("the diary answers a date it covers")
    };
    assert_eq!(ill.zone.id(), "Europe/London", "illness keeps the zone");
    assert!(
        !ill.open(date(2026, 9, 20)),
        "the Sunday of an illness is not a day the operator can train"
    );
}

/// Re-stating an absence from the same date corrects its kind too.
#[test]
fn restating_an_absence_can_make_it_illness() {
    let (store, _directory) = opened!();

    run!(store.record_alteration(&Alteration::new(
        date(2026, 9, 14),
        days!(2),
        Absence::Holiday {
            zone: None,
            slots: slots(&[(Weekday::Monday, PartOfDay::Morning, Discipline::Cycling)]),
        },
        "away".to_owned(),
    )));
    let corrected = Alteration::new(
        date(2026, 9, 14),
        days!(2),
        Absence::Illness,
        "ill, not away".to_owned(),
    );
    run!(store.record_alteration(&corrected));

    let diary = run!(store.diary());
    assert_eq!(diary.alterations(), [corrected], "one absence, corrected");
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
        Absence::Holiday {
            zone: None,
            slots: BTreeMap::new(),
        },
        "away, and unable to train".to_owned(),
    )));

    let diary = run!(store.diary());
    let lost = diary.unavailable(date(2026, 9, 14), date(2026, 9, 20), Discipline::Gym);

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
        slots(&[(Weekday::Tuesday, PartOfDay::Morning, Discipline::Gym)]),
    )));

    let diary = run!(store.diary());

    assert_eq!(diary.patterns().len(), 1, "one pattern, corrected");
    assert_eq!(
        diary.patterns()[0].slots(),
        &slots(&[(Weekday::Tuesday, PartOfDay::Morning, Discipline::Gym)]),
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
        slots(&[(Weekday::Saturday, PartOfDay::Morning, Discipline::Gym)]),
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
        slots(&[(Weekday::Saturday, PartOfDay::Morning, Discipline::Gym)]),
        "the later pattern answers from its own date"
    );

    // Before the first schedule, the operator has said nothing — which is not
    // the same as having said "no slots".
    assert!(
        diary.on(date(2026, 8, 1)).is_none(),
        "a date before the first pattern is unknown, not empty"
    );
}

/// **The case that put the allocation in this module.**
///
/// An alteration may replace the week's slots outright — a trip where the hotel
/// gym is only free at the weekend turns two weekday evenings into a Saturday
/// morning. The allocation has to move with them, and only something holding
/// both the pattern and the alterations can say so.
///
/// Were a programme handed a fixed set of its own slots instead, it would look
/// for its Monday and Friday evenings, find neither, and report the whole week
/// lost — while the Saturday it had actually been given sat unclaimed.
#[test]
fn an_alteration_moves_the_allocation_with_the_slots() {
    let (store, _directory) = opened!();

    run!(store.record_pattern(&TrainingPattern::new(
        date(2026, 8, 24),
        zone!("Europe/London"),
        ordinary_pattern()
    )));
    run!(store.record_alteration(&Alteration::new(
        date(2026, 9, 14),
        days!(7),
        Absence::Holiday {
            zone: None,
            slots: slots(&[
                (Weekday::Saturday, PartOfDay::Morning, Discipline::Gym),
                (Weekday::Sunday, PartOfDay::Morning, Discipline::Cycling),
            ]),
        },
        "away; the hotel gym is only free at the weekend".to_owned(),
    )));

    let diary = run!(store.diary());

    // The weekday evenings are gone, so those days are lost to the gym.
    assert_eq!(
        diary.unavailable(date(2026, 9, 14), date(2026, 9, 20), Discipline::Gym),
        [date(2026, 9, 14), date(2026, 9, 18)],
        "the Monday and the Friday the gym ordinarily has"
    );

    // And the Saturday it was given is there to be found, which is the half a
    // fixed allocation could never have seen.
    assert_eq!(
        diary.slots_on(date(2026, 9, 19), Discipline::Gym),
        [TrainingSlot::new(Weekday::Saturday, PartOfDay::Morning)],
        "the gym keeps the Saturday morning the alteration gave it"
    );
    assert!(
        diary
            .slots_on(date(2026, 9, 19), Discipline::Cycling)
            .is_empty(),
        "which is the gym's and not cycling's"
    );
}
