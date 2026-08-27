//! The operator's week, and the holidays that depart from it.
//!
//! The dates here are September 2026 as the operator described it: away with
//! family from Saturday 29 August through Friday 4 September, away again from
//! Friday 11 September through Monday 14 September, and in `Europe/Rome` for the
//! second of those.

use std::{collections::BTreeMap, num::NonZeroU8};

use domain::{
    gym::OperatorZone,
    schedule::{Alteration, Diary, Discipline, PartOfDay, TrainingPattern, TrainingSlot},
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

/// The operator's ordinary week: four slots, two of which are the gym's.
///
/// The allocation is part of the pattern rather than something a caller brings
/// with it, which is what lets an alteration move a slot *and* say whose the
/// new one is.
fn ordinary() -> BTreeMap<TrainingSlot, Discipline> {
    [
        (
            TrainingSlot::new(Weekday::Monday, PartOfDay::Evening),
            Discipline::Gym,
        ),
        (
            TrainingSlot::new(Weekday::Wednesday, PartOfDay::Evening),
            Discipline::Cycling,
        ),
        (
            TrainingSlot::new(Weekday::Friday, PartOfDay::Evening),
            Discipline::Gym,
        ),
        (
            TrainingSlot::new(Weekday::Sunday, PartOfDay::Morning),
            Discipline::Cycling,
        ),
    ]
    .into_iter()
    .collect()
}

fn september() -> Built<Diary> {
    let schedule = TrainingPattern::new(date(2026, 1, 1)?, zone("Europe/London")?, ordinary());

    // Away, unable to train: neither place has free weights.
    let first = Alteration::new(
        date(2026, 8, 29)?,
        days(7)?,
        None,
        Some(BTreeMap::new()),
        "away with family; no free weights where we are staying".to_owned(),
    );

    // Away, unable to train, and in another country.
    let second = Alteration::new(
        date(2026, 9, 11)?,
        days(4)?,
        Some(zone("Europe/Rome")?),
        Some(BTreeMap::new()),
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

/// **An alteration with no slots removes training without touching the zone.**
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
        "an alteration that says nothing about the zone leaves it alone"
    );
}

/// **An alteration may change both.** Rome is away *and* elsewhere.
#[test]
fn an_alteration_can_change_the_zone_and_the_slots_together() {
    let diary = september().expect("the diary builds");
    let inside = date(2026, 9, 11).expect("a real Friday");

    let availability = diary.on(inside).expect("a schedule is in force");
    assert_eq!(availability.zone.id(), "Europe/Rome");
    assert!(availability.slots.is_empty());
}

/// The day after an alteration ends is ordinary again, which is the off-by-one worth
/// pinning: a run of four days from Friday the 11th ends on Monday the 14th.
#[test]
fn an_alteration_ends_when_its_days_run_out() {
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
    let early = TrainingPattern::new(
        date(2026, 1, 1).expect("a real date"),
        zone("Europe/London").expect("a zone"),
        ordinary(),
    );
    let moved = TrainingPattern::new(
        date(2026, 10, 1).expect("a real date"),
        zone("America/New_York").expect("a zone"),
        std::iter::once((
            TrainingSlot::new(Weekday::Tuesday, PartOfDay::Morning),
            Discipline::Gym,
        ))
        .collect(),
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

/// **What a programme consults, and it names itself rather than its slots.**
///
/// Wednesday evening and Sunday morning are cycling's, so a gym block losing
/// them is not the gym's problem — and a gym block that counted them would skip
/// weeks it never planned to run.
#[test]
fn a_programme_loses_only_its_own_days() {
    let diary = september().expect("the diary builds");

    let lost = diary.unavailable(
        date(2026, 8, 24).expect("a real date"),
        date(2026, 9, 20).expect("a real date"),
        Discipline::Gym,
    );

    let expected = vec![
        date(2026, 8, 31).expect("Monday inside the first absence"),
        date(2026, 9, 4).expect("Friday inside the first absence"),
        date(2026, 9, 11).expect("Friday inside the second"),
        date(2026, 9, 14).expect("Monday inside the second"),
    ];
    assert_eq!(lost, expected);
}

/// The same range asked for cycling loses different days, which is the point of
/// the allocation living here: each discipline sees only its own.
#[test]
fn each_discipline_loses_its_own_days() {
    let diary = september().expect("the diary builds");

    let gym = diary.unavailable(
        date(2026, 8, 24).expect("a real date"),
        date(2026, 9, 20).expect("a real date"),
        Discipline::Gym,
    );
    let cycling = diary.unavailable(
        date(2026, 8, 24).expect("a real date"),
        date(2026, 9, 20).expect("a real date"),
        Discipline::Cycling,
    );

    assert_ne!(gym, cycling, "the two disciplines lose different days");
    assert!(
        !gym.iter().any(|date| cycling.contains(date)),
        "no day is lost by both: they train on different days"
    );
}

/// **A day is lost when the allocated slot is gone, not when the day empties.**
///
/// The operator trains Monday morning and Monday evening; the gym has been
/// allocated the evening. An alteration leaves the morning and takes the rest — he
/// trains, then goes away at lunchtime. The Monday is lost to the gym even
/// though the day is not empty.
///
/// Asking `Availability::open` answers "could he train at all", which is a
/// different question: it read the surviving morning as a surviving evening and
/// reported nothing lost. That is the whole reason a slot carries a part of the
/// day rather than only a weekday.
#[test]
fn a_day_that_keeps_the_wrong_half_is_still_lost() {
    let monday = date(2026, 9, 14).expect("a real Monday");

    let ordinary: BTreeMap<TrainingSlot, Discipline> = [
        (
            TrainingSlot::new(Weekday::Monday, PartOfDay::Morning),
            Discipline::Cycling,
        ),
        (
            TrainingSlot::new(Weekday::Monday, PartOfDay::Evening),
            Discipline::Gym,
        ),
    ]
    .into_iter()
    .collect();

    let morning_only: BTreeMap<TrainingSlot, Discipline> = std::iter::once((
        TrainingSlot::new(Weekday::Monday, PartOfDay::Morning),
        Discipline::Cycling,
    ))
    .collect();

    let diary = Diary::new(
        vec![TrainingPattern::new(
            date(2026, 9, 7).expect("a real date"),
            zone("Europe/London").expect("a real zone"),
            ordinary,
        )],
        vec![Alteration::new(
            monday,
            days(1).expect("one day"),
            None,
            Some(morning_only),
            "trains in the morning, away from lunchtime".to_owned(),
        )],
    );

    assert_eq!(
        diary.unavailable(monday, monday, Discipline::Gym),
        vec![monday],
        "the gym's evening is gone, so the gym loses the day"
    );
    assert_eq!(
        diary.unavailable(monday, monday, Discipline::Cycling),
        Vec::new(),
        "cycling keeps the morning it was allocated"
    );
    assert_eq!(
        diary.slots_on(monday, Discipline::Cycling),
        vec![TrainingSlot::new(Weekday::Monday, PartOfDay::Morning)],
        "and can say what it still has"
    );
}

/// **Each discipline's ordinary week is its own.** The weekly shape a programme
/// takes is the days the schedule allocated to it, and nothing else.
#[test]
fn the_ordinary_week_names_one_disciplines_days() {
    let diary = september().expect("the diary builds");
    let inside_the_block = date(2026, 9, 21).expect("a real Monday");

    assert_eq!(
        diary.ordinarily(inside_the_block, Discipline::Gym),
        Some(vec![Weekday::Monday, Weekday::Friday]),
        "the gym has Monday and Friday, Monday first"
    );
    assert_eq!(
        diary.ordinarily(inside_the_block, Discipline::Cycling),
        Some(vec![Weekday::Wednesday, Weekday::Sunday]),
        "and cycling has the other two"
    );
}

/// **An alteration interrupts a block; it does not reshape it.**
///
/// The autumn block starts on 2026-09-14, which is the last day of the Rome
/// alteration and has no room to train at all. Reading the altered week there
/// would give the block no days and no heavy session — a holiday deciding the
/// shape of the fourteen weeks after it. The loss is taken separately, as a
/// skip.
#[test]
fn an_alteration_does_not_reshape_the_ordinary_week() {
    let diary = september().expect("the diary builds");
    let starts = date(2026, 9, 14).expect("a real Monday");

    let availability = diary.on(starts).expect("a schedule is in force");
    assert!(
        !availability.open(starts),
        "the day itself holds no training"
    );
    assert_eq!(
        diary.unavailable(starts, starts, Discipline::Gym),
        vec![starts],
        "so the gym loses it"
    );

    assert_eq!(
        diary.ordinarily(starts, Discipline::Gym),
        Some(vec![Weekday::Monday, Weekday::Friday]),
        "and the ordinary week is untouched by it"
    );
}

/// **Two slots on one day are one training day.** A weekday appears once
/// however many parts of it a discipline holds, because a programme's weekly
/// shape counts days rather than slots.
#[test]
fn a_day_held_twice_is_named_once() {
    let both_halves = [
        (
            TrainingSlot::new(Weekday::Saturday, PartOfDay::Morning),
            Discipline::Gym,
        ),
        (
            TrainingSlot::new(Weekday::Saturday, PartOfDay::Evening),
            Discipline::Gym,
        ),
    ]
    .into_iter()
    .collect();
    let from = date(2026, 1, 1).expect("a real date");
    let diary = Diary::new(
        vec![TrainingPattern::new(
            from,
            zone("Europe/London").expect("a real zone"),
            both_halves,
        )],
        vec![],
    );

    assert_eq!(
        diary.ordinarily(from, Discipline::Gym),
        Some(vec![Weekday::Saturday])
    );
}

/// **Unknown and empty are different answers**, as they are for `on`.
#[test]
fn a_week_nobody_has_described_has_no_ordinary_days() {
    let diary = september().expect("the diary builds");
    let before = date(2025, 12, 31).expect("a real date");

    assert_eq!(
        diary.ordinarily(before, Discipline::Gym),
        None,
        "nothing has been said about this week"
    );

    let empty = Diary::new(
        vec![TrainingPattern::new(
            date(2026, 1, 1).expect("a real date"),
            zone("Europe/London").expect("a real zone"),
            BTreeMap::new(),
        )],
        vec![],
    );
    assert_eq!(
        empty.ordinarily(date(2026, 1, 1).expect("a real date"), Discipline::Gym),
        Some(Vec::new()),
        "this week is described, and holds nothing for the gym"
    );
}
