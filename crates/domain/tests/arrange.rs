//! Which discipline and which session each free slot takes (#673), on the
//! weeks the operator gave on 2026-10-31.

use domain::planner::Arrangement as _;
use std::{collections::BTreeMap, num::NonZeroU8};

use domain::{
    normalised::OperatorZone,
    planner::{Discipline, Limits, Relative, arrange},
    schedule::{Alteration, Diary, PartOfDay, SlotLength, TrainingPattern, TrainingSlot},
};
use jiff::civil::{Date, Weekday};

use PartOfDay::{Afternoon, Evening, Morning};
use SlotLength::{Longer, Shorter};
use Weekday::{Friday, Monday, Saturday, Sunday, Thursday, Tuesday, Wednesday};

fn free(slots: &[(Weekday, PartOfDay, SlotLength)]) -> BTreeMap<TrainingSlot, SlotLength> {
    slots
        .iter()
        .map(|(day, part, length)| (TrainingSlot::new(*day, *part), *length))
        .collect()
}

/// A week as the operator says it: "Monday evening: gym, easier".
fn said(free: &BTreeMap<TrainingSlot, SlotLength>) -> Vec<String> {
    arrange(free, Limits::STATED)
        .into_iter()
        .map(|(slot, (discipline, role))| {
            let session = match (
                discipline,
                role.map(domain::planner::SessionRole::intensity),
            ) {
                (Discipline::Cycling, Some(Relative::Lower)) => "long ride",
                (Discipline::Cycling, _) => "ride, harder",
                (Discipline::Gym, Some(Relative::Lower)) => "gym, easier",
                (Discipline::Gym, Some(Relative::Higher)) => "gym, harder",
                (Discipline::Gym, None) => "gym",
            };
            format!("{:?} {}: {session}", slot.weekday, slot.part)
        })
        .collect()
}

/// The ordinary week: Monday, Wednesday and Friday evenings, and the longer
/// Sunday morning.
fn ordinary() -> BTreeMap<TrainingSlot, SlotLength> {
    free(&[
        (Monday, Evening, Shorter),
        (Wednesday, Evening, Shorter),
        (Friday, Evening, Shorter),
        (Sunday, Morning, Longer),
    ])
}

#[test]
fn the_ordinary_week_comes_out_as_it_is_stated_now() {
    assert_eq!(
        said(&ordinary()),
        [
            "Monday evening: gym, easier",
            "Wednesday evening: ride, harder",
            "Friday evening: gym, harder",
            "Sunday morning: long ride",
        ]
    );
}

#[test]
fn friday_moved_to_saturday_takes_the_harder_gym_session_with_it() {
    let week = free(&[
        (Monday, Evening, Shorter),
        (Wednesday, Evening, Shorter),
        (Saturday, Evening, Shorter),
        (Sunday, Morning, Longer),
    ]);
    assert_eq!(
        said(&week),
        [
            "Monday evening: gym, easier",
            "Wednesday evening: ride, harder",
            "Saturday evening: gym, harder",
            "Sunday morning: long ride",
        ]
    );
}

#[test]
fn friday_lost_gives_sunday_to_the_harder_gym_session() {
    let week = free(&[
        (Monday, Evening, Shorter),
        (Wednesday, Evening, Shorter),
        (Sunday, Morning, Longer),
    ]);
    assert_eq!(
        said(&week),
        [
            "Monday evening: gym, easier",
            "Wednesday evening: ride, harder",
            "Sunday morning: gym, harder",
        ]
    );
}

/// Three gym sessions alternate their primaries, which is #676's split: the
/// schedule says which are the gym's and gives them no role.
#[test]
fn five_slots_put_the_long_ride_on_saturday() {
    let week = free(&[
        (Monday, Evening, Shorter),
        (Wednesday, Evening, Shorter),
        (Friday, Evening, Shorter),
        (Saturday, Morning, Longer),
        (Sunday, Morning, Longer),
    ]);
    assert_eq!(
        said(&week),
        [
            "Monday evening: gym",
            "Wednesday evening: ride, harder",
            "Friday evening: gym",
            "Saturday morning: long ride",
            "Sunday morning: gym",
        ]
    );
}

/// The operator, 2026-10-31: *"in an ideal world, a completely free week, it
/// would be gym - cycling - gym - cycling - gym - cycling - rest all
/// mornings."*
#[test]
fn a_completely_free_week_is_six_mornings_and_a_rest_on_sunday() {
    let mut week = BTreeMap::new();
    for day in [
        Monday, Tuesday, Wednesday, Thursday, Friday, Saturday, Sunday,
    ] {
        let weekend = matches!(day, Saturday | Sunday);
        week.insert(
            TrainingSlot::new(day, Morning),
            if weekend { Longer } else { Shorter },
        );
        week.insert(TrainingSlot::new(day, Evening), Shorter);
    }
    assert_eq!(
        said(&week),
        [
            "Monday morning: gym",
            "Tuesday morning: ride, harder",
            "Wednesday morning: gym",
            "Thursday morning: ride, harder",
            "Friday morning: gym",
            "Saturday morning: long ride",
        ]
    );
}

/// The operator's own example: with Friday and Sunday evenings fixed, Saturday
/// evening has a morning and an afternoon either side of it.
#[test]
fn a_day_takes_the_slot_that_keeps_its_neighbours_furthest_away() {
    let week = free(&[
        (Friday, Evening, Shorter),
        (Saturday, Morning, Longer),
        (Saturday, Afternoon, Longer),
        (Saturday, Evening, Longer),
        (Sunday, Evening, Shorter),
    ]);
    let used: Vec<TrainingSlot> = arrange(&week, Limits::STATED).into_keys().collect();
    assert!(
        used.contains(&TrainingSlot::new(Saturday, Evening)),
        "Saturday evening, not {used:?}"
    );
}

type Built<T> = Result<T, Box<dyn std::error::Error>>;

const fn date(year: i16, month: i8, day: i8) -> Date {
    Date::constant(year, month, day)
}

fn away(start: Date, days: u8, slots: BTreeMap<TrainingSlot, SlotLength>) -> Built<Alteration> {
    let days = NonZeroU8::new(days).ok_or("a run of days is at least one")?;
    Ok(Alteration::new(
        start,
        days,
        None,
        Some(slots),
        "away".to_owned(),
    ))
}

fn diary(alterations: Vec<Alteration>) -> Built<Diary> {
    let zone = OperatorZone::try_from("Europe/London")?;
    Ok(Diary::new(
        vec![TrainingPattern::new(date(2026, 1, 1), zone, ordinary())],
        alterations,
    ))
}

/// Away Tuesday 3 to Friday 6 November, leaving Monday evening and Sunday
/// morning: the harder gym session and the harder ride, so the mesocycle moves
/// forward and nothing is lost.
#[test]
fn a_holiday_rearranges_what_its_week_has_left() {
    let diary = diary(vec![
        away(date(2026, 11, 3), 4, BTreeMap::new()).expect("a trip"),
    ])
    .expect("the diary builds");
    let week: Vec<String> = (2..=8)
        .flat_map(|day| diary.planned_slots_of(date(2026, 11, day)))
        .map(|slot| format!("{} {}", slot.discipline, slot.date))
        .collect();
    assert_eq!(week, ["gym 2026-11-02", "cycling 2026-11-08"]);
    let roles: Vec<_> = (2..=8)
        .flat_map(|day| diary.planned_slots_of(date(2026, 11, day)))
        .map(|slot| slot.role.map(domain::planner::SessionRole::intensity))
        .collect();
    assert_eq!(roles, [Some(Relative::Higher), Some(Relative::Higher)]);
    assert!(diary.lost_disciplines(date(2026, 11, 2)).is_empty());
}

/// Lose one more day and the week holds only the essential gym session:
/// cycling's is lost, and the microcycle runs again.
#[test]
fn a_week_that_cannot_be_rearranged_loses_a_discipline() {
    let diary = diary(vec![
        away(date(2026, 11, 3), 6, BTreeMap::new()).expect("a trip"),
    ])
    .expect("the diary builds");
    let week: Vec<String> = (2..=8)
        .flat_map(|day| diary.planned_slots_of(date(2026, 11, day)))
        .map(|slot| format!("{} {}", slot.discipline, slot.date))
        .collect();
    assert_eq!(week, ["gym 2026-11-02"]);
    assert_eq!(
        diary.lost_disciplines(date(2026, 11, 2)),
        [Discipline::Cycling]
    );
}

/// A holiday stating Saturday evening in place of Friday's is one changed slot.
#[test]
fn a_holiday_that_moves_a_slot_moves_its_session() {
    let diary = diary(vec![
        away(date(2026, 11, 6), 2, free(&[(Saturday, Evening, Shorter)])).expect("a trip"),
    ])
    .expect("the diary builds");
    let saturday = diary.planned_slots_of(date(2026, 11, 7));
    assert_eq!(saturday.len(), 1, "{saturday:?}");
    assert!(
        saturday
            .iter()
            .all(|slot| slot.discipline == Discipline::Gym
                && slot.role.map(domain::planner::SessionRole::intensity)
                    == Some(Relative::Higher))
    );
    assert!(diary.planned_slots_of(date(2026, 11, 6)).is_empty());
}
