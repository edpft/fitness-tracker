//! The operator's week, and the holidays that depart from it.
//!
//! The dates here are September 2026 as the operator described it: away with
//! family from Saturday 29 August through Friday 4 September, away again from
//! Friday 11 September through Monday 14 September, and in `Europe/Rome` for the
//! second of those.

use std::{collections::BTreeMap, num::NonZeroU8};

use domain::{
    normalised::OperatorZone,
    schedule::{
        Absence, Allocation, Alteration, Diary, Discipline, PartOfDay, Relative, ScheduledSlot,
        SessionRole, TrainingPattern, TrainingSlot, unaccounted,
    },
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

/// The harder, shorter session of a discipline's week.
const fn harder() -> SessionRole {
    SessionRole::new(Relative::Higher, Relative::Lower)
}

/// The easier, longer one.
const fn easier() -> SessionRole {
    SessionRole::new(Relative::Lower, Relative::Higher)
}

const fn gym(role: SessionRole) -> Allocation {
    Allocation::new(Discipline::Gym, role)
}

const fn cycling(role: SessionRole) -> Allocation {
    Allocation::new(Discipline::Cycling, role)
}

/// The operator's ordinary week: four slots, two of which are the gym's.
///
/// The allocation is part of the pattern rather than something a caller brings
/// with it, which is what lets an alteration move a slot *and* say whose the
/// new one is — and, since 2026-09-20, what session it takes.
fn ordinary() -> BTreeMap<TrainingSlot, Allocation> {
    [
        (
            TrainingSlot::new(Weekday::Monday, PartOfDay::Evening),
            gym(easier()),
        ),
        (
            TrainingSlot::new(Weekday::Wednesday, PartOfDay::Evening),
            cycling(harder()),
        ),
        (
            TrainingSlot::new(Weekday::Friday, PartOfDay::Evening),
            gym(harder()),
        ),
        (
            TrainingSlot::new(Weekday::Sunday, PartOfDay::Morning),
            cycling(easier()),
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
        Absence::Holiday {
            zone: None,
            slots: Some(BTreeMap::new()),
            reason: "away with family; no free weights where we are staying".to_owned(),
        },
    );

    // Away, unable to train, and in another country.
    let second = Alteration::new(
        date(2026, 9, 11)?,
        days(4)?,
        Absence::Holiday {
            zone: Some(zone("Europe/Rome")?),
            slots: Some(BTreeMap::new()),
            reason: "away with family in Rome".to_owned(),
        },
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
            gym(harder()),
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

    let ordinary: BTreeMap<TrainingSlot, Allocation> = [
        (
            TrainingSlot::new(Weekday::Monday, PartOfDay::Morning),
            cycling(easier()),
        ),
        (
            TrainingSlot::new(Weekday::Monday, PartOfDay::Evening),
            gym(harder()),
        ),
    ]
    .into_iter()
    .collect();

    let morning_only: BTreeMap<TrainingSlot, Allocation> = std::iter::once((
        TrainingSlot::new(Weekday::Monday, PartOfDay::Morning),
        cycling(easier()),
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
            Absence::Holiday {
                zone: None,
                slots: Some(morning_only),
                reason: "trains in the morning, away from lunchtime".to_owned(),
            },
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
        vec![(
            TrainingSlot::new(Weekday::Monday, PartOfDay::Morning),
            easier()
        )],
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
            gym(easier()),
        ),
        (
            TrainingSlot::new(Weekday::Saturday, PartOfDay::Evening),
            gym(harder()),
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

/// One slot on a date, with the role the ordinary week gives it.
///
/// **The role is looked up where the week has one**, rather than stated: a
/// second copy of the allocation here would be exactly the duplication issue
/// #63 removed. A day the ordinary week does not hold — a Saturday an
/// alteration handed over — falls back to the harder session, and nothing
/// these tests assert turns on which it is.
fn slot(on: Date, part: PartOfDay, discipline: Discipline) -> ScheduledSlot {
    let slot = TrainingSlot::new(on.weekday(), part);
    let role = ordinary()
        .get(&slot)
        .filter(|allocated| allocated.discipline == discipline)
        .map_or_else(harder, |allocated| allocated.role);
    ScheduledSlot {
        date: on,
        slot,
        discipline,
        role,
    }
}

/// **The previous slot is found across an absence.** Rome takes Friday 11
/// through Monday 14, so on Wednesday 16 the last slot there was room for is
/// the Wednesday before.
#[test]
fn the_last_slot_before_a_day_skips_the_days_an_absence_took() {
    let diary = september().expect("the diary builds");
    let wednesday = date(2026, 9, 16).expect("a real Wednesday");

    assert_eq!(
        diary.last_before(wednesday),
        Some(slot(
            date(2026, 9, 9).expect("a real Wednesday"),
            PartOfDay::Evening,
            Discipline::Cycling,
        )),
    );
}

/// And the next one is found across it the other way.
#[test]
fn the_first_slot_after_a_day_skips_the_days_an_absence_took() {
    let diary = september().expect("the diary builds");
    let thursday = date(2026, 9, 10).expect("a real Thursday");

    assert_eq!(
        diary.first_after(thursday),
        Some(slot(
            date(2026, 9, 16).expect("a real Wednesday"),
            PartOfDay::Evening,
            Discipline::Cycling,
        )),
    );
}

/// **Due by a day is the last slot before it and every slot on it.**
#[test]
fn what_is_due_by_a_day_is_the_slot_before_and_the_day_itself() {
    let diary = september().expect("the diary builds");
    let monday = date(2026, 9, 21).expect("a real Monday");

    assert_eq!(
        diary.due_by(monday),
        vec![
            slot(
                date(2026, 9, 20).expect("a real Sunday"),
                PartOfDay::Morning,
                Discipline::Cycling,
            ),
            slot(monday, PartOfDay::Evening, Discipline::Gym),
        ],
    );
}

/// **A diary with no slot in it has no next slot**, and says so rather than
/// searching for ever.
#[test]
fn a_week_with_no_slots_has_no_next_slot() {
    let start = date(2026, 1, 1).expect("a real date");
    let empty = Diary::new(
        vec![TrainingPattern::new(
            start,
            zone("Europe/London").expect("a real zone"),
            BTreeMap::new(),
        )],
        vec![],
    );

    assert_eq!(empty.first_after(start), None);
    assert_eq!(empty.last_before(start), None);
    assert_eq!(Diary::default().first_after(start), None);
}

/// **A session done late accounts for the slot it was late for.** The
/// operator's example, 2026-09-19: the gym entry test missed through illness
/// yesterday and performed today is that test.
#[test]
fn a_session_performed_after_its_slot_accounts_for_it() {
    let friday = date(2026, 9, 18).expect("a real Friday");
    let saturday = date(2026, 9, 19).expect("a real Saturday");

    let due = [slot(friday, PartOfDay::Evening, Discipline::Gym)];
    let performed = BTreeMap::from([(Discipline::Gym, vec![saturday])]);

    assert_eq!(unaccounted(&due, &performed), vec![]);
}

/// **One session answers for one slot.** Done a day late, it fills the slot
/// it was late for, and the day's own slot is still to do.
#[test]
fn one_session_accounts_for_one_slot() {
    let friday = date(2026, 9, 18).expect("a real Friday");
    let saturday = date(2026, 9, 19).expect("a real Saturday");

    let due = [
        slot(saturday, PartOfDay::Morning, Discipline::Gym),
        slot(friday, PartOfDay::Evening, Discipline::Gym),
    ];
    let performed = BTreeMap::from([(Discipline::Gym, vec![saturday])]);

    assert_eq!(
        unaccounted(&due, &performed),
        vec![slot(saturday, PartOfDay::Morning, Discipline::Gym)],
    );
}

/// **A session before the slot does not account for it**, and nor does one of
/// another discipline.
#[test]
fn a_session_before_its_slot_or_of_another_discipline_does_not_count() {
    let wednesday = date(2026, 9, 16).expect("a real Wednesday");
    let tuesday = date(2026, 9, 15).expect("a real Tuesday");
    let thursday = date(2026, 9, 17).expect("a real Thursday");

    let due = [slot(wednesday, PartOfDay::Evening, Discipline::Cycling)];
    let performed = BTreeMap::from([
        (Discipline::Cycling, vec![tuesday]),
        (Discipline::Gym, vec![thursday]),
    ]);

    assert_eq!(unaccounted(&due, &performed), due.to_vec());
}

/// Illness leaves no room to train and says nothing about where the operator
/// is, so an illness in the middle of a holiday keeps the holiday's zone.
#[test]
fn illness_empties_the_day_and_keeps_the_zone() {
    let rome = zone("Europe/Rome").expect("a real zone");
    let saturday = date(2026, 9, 12).expect("a real date");

    let mut alterations = september()
        .expect("the diary builds")
        .alterations()
        .to_vec();
    alterations.push(Alteration::new(
        saturday,
        days(1).expect("one day"),
        Absence::Illness,
    ));
    let diary = Diary::new(
        vec![TrainingPattern::new(
            date(2026, 1, 1).expect("a real date"),
            zone("Europe/London").expect("a real zone"),
            ordinary(),
        )],
        alterations,
    );

    let ill = diary.on(saturday).expect("a schedule is in force");
    assert_eq!(ill.zone, rome, "still in Rome");
    assert!(ill.slots.is_empty(), "no room to train while ill");
    assert_eq!(diary.alterations()[2].slots(), Some(&BTreeMap::new()));
}
