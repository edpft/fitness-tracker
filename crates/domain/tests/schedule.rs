//! The operator's week, and the holidays that depart from it.
//!
//! The dates here are September 2026 as the operator described it: away with
//! family from Saturday 29 August through Friday 4 September, away again from
//! Friday 11 September through Monday 14 September, and in `Europe/Rome` for the
//! second of those.

use std::{collections::BTreeSet, num::NonZeroU8};

use domain::{
    gym::OperatorZone,
    schedule::{Diary, PartOfDay, Patch, Schedule, Slot},
};
use jiff::civil::{Date, Weekday};

type Built<T> = Result<T, Box<dyn std::error::Error>>;

fn date(year: i16, month: i8, day: i8) -> Built<Date> {
    Ok(Date::new(year, month, day)?)
}

fn zone(id: &str) -> Built<OperatorZone> {
    Ok(OperatorZone::try_from(id)?)
}

fn days(count: u8) -> Built<NonZeroU8> {
    NonZeroU8::new(count).ok_or_else(|| "a run of days is at least one".into())
}

/// The operator's ordinary week: four slots, two of which the gym uses.
fn ordinary() -> BTreeSet<Slot> {
    [
        Slot::new(Weekday::Monday, PartOfDay::Evening),
        Slot::new(Weekday::Wednesday, PartOfDay::Evening),
        Slot::new(Weekday::Friday, PartOfDay::Evening),
        Slot::new(Weekday::Sunday, PartOfDay::Morning),
    ]
    .into_iter()
    .collect()
}

/// What the gym has been allocated. **Not the whole pool** — Wednesday evening
/// and Sunday morning belong to cycling.
fn allocated_to_the_gym() -> BTreeSet<Slot> {
    [
        Slot::new(Weekday::Monday, PartOfDay::Evening),
        Slot::new(Weekday::Friday, PartOfDay::Evening),
    ]
    .into_iter()
    .collect()
}

fn september() -> Built<Diary> {
    let schedule = Schedule::new(date(2026, 1, 1)?, zone("Europe/London")?, ordinary());

    // Away, unable to train: neither place has free weights.
    let first = Patch::new(
        date(2026, 8, 29)?,
        days(7)?,
        None,
        Some(BTreeSet::new()),
        "away with family; no free weights where we are staying".to_owned(),
    );

    // Away, unable to train, and in another country.
    let second = Patch::new(
        date(2026, 9, 11)?,
        days(4)?,
        Some(zone("Europe/Rome")?),
        Some(BTreeSet::new()),
        "away with family in Rome".to_owned(),
    );

    Ok(Diary::new(vec![schedule], vec![first, second]))
}

/// An ordinary week answers with the ordinary zone and the ordinary slots.
#[test]
fn an_ordinary_day_reads_the_schedule_in_force() {
    let diary = september().expect("the diary builds");
    let monday = date(2026, 8, 24).expect("a real Monday");

    let availability = diary.on(monday).expect("a schedule is in force");
    assert_eq!(availability.zone.id(), "Europe/London");
    assert_eq!(availability.slots, ordinary());
    assert!(availability.open(monday), "Monday evening is a slot");
}

/// **A patch with no slots removes training without touching the zone.**
#[test]
fn a_hard_absence_closes_the_days_it_covers() {
    let diary = september().expect("the diary builds");
    let inside = date(2026, 8, 31).expect("a real Monday");

    let availability = diary.on(inside).expect("a schedule is in force");
    assert!(availability.slots.is_empty());
    assert!(!availability.open(inside));
    assert_eq!(
        availability.zone.id(),
        "Europe/London",
        "a patch that says nothing about the zone leaves it alone"
    );
}

/// **A patch may change both.** Rome is away *and* elsewhere.
#[test]
fn a_patch_can_change_the_zone_and_the_slots_together() {
    let diary = september().expect("the diary builds");
    let inside = date(2026, 9, 11).expect("a real Friday");

    let availability = diary.on(inside).expect("a schedule is in force");
    assert_eq!(availability.zone.id(), "Europe/Rome");
    assert!(availability.slots.is_empty());
}

/// The day after a patch ends is ordinary again, which is the off-by-one worth
/// pinning: a run of four days from Friday the 11th ends on Monday the 14th.
#[test]
fn a_patch_ends_when_its_days_run_out() {
    let diary = september().expect("the diary builds");

    let last = date(2026, 9, 14).expect("a real Monday");
    let after = date(2026, 9, 15).expect("a real Tuesday");

    assert_eq!(
        diary.on(last).expect("in force").zone.id(),
        "Europe/Rome",
        "the fourth day is still covered"
    );
    assert_eq!(
        diary.on(after).expect("in force").zone.id(),
        "Europe/London",
        "the fifth is not"
    );
}

/// **Before the first schedule, nothing is known.** A date the operator has said
/// nothing about is unknown rather than empty, and inventing a week for it would
/// assert a fact nobody stated.
#[test]
fn a_date_before_any_schedule_is_unknown() {
    let diary = september().expect("the diary builds");
    assert_eq!(diary.on(date(2025, 6, 1).expect("a real date")), None);
}

/// A later schedule supersedes an earlier one, and the earlier still answers for
/// its own dates.
#[test]
fn a_later_schedule_supersedes_an_earlier_one() {
    let early = Schedule::new(
        date(2026, 1, 1).expect("a real date"),
        zone("Europe/London").expect("a zone"),
        ordinary(),
    );
    let moved = Schedule::new(
        date(2026, 10, 1).expect("a real date"),
        zone("America/New_York").expect("a zone"),
        std::iter::once(Slot::new(Weekday::Tuesday, PartOfDay::Morning)).collect(),
    );
    let diary = Diary::new(vec![moved, early], vec![]);

    assert_eq!(
        diary
            .on(date(2026, 9, 30).expect("a real date"))
            .expect("in force")
            .zone
            .id(),
        "Europe/London"
    );
    assert_eq!(
        diary
            .on(date(2026, 10, 1).expect("a real date"))
            .expect("in force")
            .zone
            .id(),
        "America/New_York",
        "the day it begins, not the day after"
    );
}

/// **What a programme consults, and it asks about its own slots only.**
///
/// Wednesday evening and Sunday morning belong to cycling, so a gym block losing
/// them is not the gym's problem — and a gym block that counted them would skip
/// weeks it never planned to run.
#[test]
fn a_programme_loses_only_the_days_it_was_allocated() {
    let diary = september().expect("the diary builds");

    let lost = diary.unavailable(
        date(2026, 8, 24).expect("a real date"),
        date(2026, 9, 20).expect("a real date"),
        &allocated_to_the_gym(),
    );

    let expected = vec![
        date(2026, 8, 31).expect("Monday inside the first absence"),
        date(2026, 9, 4).expect("Friday inside the first absence"),
        date(2026, 9, 11).expect("Friday inside the second"),
        date(2026, 9, 14).expect("Monday inside the second"),
    ];
    assert_eq!(lost, expected);
}

/// The same range against the whole pool loses more, which is exactly why a
/// programme must be told its allocation rather than reading the pool.
#[test]
fn the_whole_pool_loses_more_than_the_gym_does() {
    let diary = september().expect("the diary builds");

    let gym = diary.unavailable(
        date(2026, 8, 24).expect("a real date"),
        date(2026, 9, 20).expect("a real date"),
        &allocated_to_the_gym(),
    );
    let everything = diary.unavailable(
        date(2026, 8, 24).expect("a real date"),
        date(2026, 9, 20).expect("a real date"),
        &ordinary(),
    );

    assert!(
        everything.len() > gym.len(),
        "cycling's slots are lost too: {everything:?} against {gym:?}"
    );
}

/// **A day is lost when the allocated slot is gone, not when the day empties.**
///
/// The operator trains Monday morning and Monday evening; the gym has been
/// allocated the evening. A patch leaves the morning and takes the rest — he
/// trains, then goes away at lunchtime. The Monday is lost to the gym even
/// though the day is not empty.
///
/// Asking `Availability::open` answers "could he train at all", which is a
/// different question: it read the surviving morning as a surviving evening and
/// reported nothing lost. That is the whole reason a slot carries a part of the
/// day rather than only a weekday.
#[test]
fn a_day_that_keeps_the_wrong_half_is_still_lost() -> Built<()> {
    let monday = date(2026, 9, 14)?;

    let ordinary: BTreeSet<Slot> = [
        Slot::new(Weekday::Monday, PartOfDay::Morning),
        Slot::new(Weekday::Monday, PartOfDay::Evening),
    ]
    .into_iter()
    .collect();

    let morning_only: BTreeSet<Slot> = [Slot::new(Weekday::Monday, PartOfDay::Morning)]
        .into_iter()
        .collect();

    let diary = Diary::new(
        vec![Schedule::new(
            date(2026, 9, 7)?,
            zone("Europe/London")?,
            ordinary,
        )],
        vec![Patch::new(
            monday,
            days(1)?,
            None,
            Some(morning_only),
            "trains in the morning, away from lunchtime".to_owned(),
        )],
    );

    let evening: BTreeSet<Slot> = [Slot::new(Weekday::Monday, PartOfDay::Evening)]
        .into_iter()
        .collect();
    let morning: BTreeSet<Slot> = [Slot::new(Weekday::Monday, PartOfDay::Morning)]
        .into_iter()
        .collect();

    assert_eq!(
        diary.unavailable(monday, monday, &evening),
        vec![monday],
        "the evening is gone, so a programme holding it loses the day"
    );
    assert_eq!(
        diary.unavailable(monday, monday, &morning),
        Vec::new(),
        "a programme holding the morning keeps it"
    );

    Ok(())
}
