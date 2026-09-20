//! Issue #185's acceptance: where each session of a microcycle stands.
//!
//! The week is the operator's own, 14 to 20 September 2026, as his store holds
//! it: Monday and Friday evenings the gym's, Wednesday evening and Sunday
//! morning cycling's, a holiday covering the 11th to the 14th, and an illness
//! covering the 17th and 18th. The gym's Friday session was prescribed on the
//! 15th and delivered to Hevy; the Wednesday ride was ridden.
//!
//! **The states are read from a window, not from a calendar.** A session may be
//! performed at any point between its own slot and the start of the next one,
//! counted in parts of a day, and an absence takes those parts away. That is
//! the whole of why the same week reports differently on a Saturday evening and
//! on the Sunday morning, and both readings are the operator's own.

use std::{collections::BTreeMap, num::NonZeroU8};

use domain::{
    normalised::OperatorZone,
    planner::{Recorded, SessionState, state_of},
    schedule::{
        Absence, AbsenceKind, Allocation, Alteration, DayPart, Diary, Discipline, PartOfDay,
        Relative, SessionRole, TrainingPattern, TrainingSlot,
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

const fn harder() -> SessionRole {
    SessionRole::new(Relative::Higher, Relative::Lower)
}

const fn easier() -> SessionRole {
    SessionRole::new(Relative::Lower, Relative::Higher)
}

/// The operator's ordinary week, as `fitness schedule show` prints it.
fn ordinary() -> BTreeMap<TrainingSlot, Allocation> {
    [
        (
            TrainingSlot::new(Weekday::Monday, PartOfDay::Evening),
            Allocation::new(Discipline::Gym, easier()),
        ),
        (
            TrainingSlot::new(Weekday::Wednesday, PartOfDay::Evening),
            Allocation::new(Discipline::Cycling, harder()),
        ),
        (
            TrainingSlot::new(Weekday::Friday, PartOfDay::Evening),
            Allocation::new(Discipline::Gym, harder()),
        ),
        (
            TrainingSlot::new(Weekday::Sunday, PartOfDay::Morning),
            Allocation::new(Discipline::Cycling, easier()),
        ),
    ]
    .into_iter()
    .collect()
}

/// The diary as his store held it on 20 September 2026, less the illness.
fn september() -> Built<Diary> {
    let pattern = TrainingPattern::new(date(2026, 8, 25)?, zone("Europe/London")?, ordinary());

    let rome = Alteration::new(
        date(2026, 9, 11)?,
        days(4)?,
        Absence::Holiday {
            zone: Some(zone("Europe/Rome")?),
            slots: Some(BTreeMap::new()),
            reason: "No holiday, no gym access".to_owned(),
        },
    );

    Ok(Diary::new(vec![pattern], vec![rome]))
}

/// The same diary, with an illness over a run of days.
fn september_ill_from(start: Date, over: NonZeroU8) -> Built<Diary> {
    let diary = september()?;
    Ok(Diary::new(
        diary.patterns().to_vec(),
        diary
            .alterations()
            .iter()
            .cloned()
            .chain(std::iter::once(Alteration::new(
                start,
                over,
                Absence::Illness,
            )))
            .collect(),
    ))
}

const fn at(on: Date, part: PartOfDay) -> DayPart {
    DayPart::new(on, part)
}

const NEITHER: Recorded = Recorded {
    prescribed: false,
    performed: false,
};

const PRESCRIBED: Recorded = Recorded {
    prescribed: true,
    performed: false,
};

const PERFORMED: Recorded = Recorded {
    prescribed: true,
    performed: true,
};

/// The four sessions of the week of 14 September, run on the Saturday evening.
///
/// #185's acceptance list, before the illness was recorded: gym 1 skipped for
/// the holiday, the Wednesday ride performed, Friday's gym session prescribed
/// with an evening left to do it in, and Sunday's ride the one to deliver.
#[test]
fn the_week_of_14_september_reads_as_the_operator_described_it() {
    let diary = september().expect("the diary builds");
    let monday = date(2026, 9, 14).expect("a real Monday");
    let wednesday = date(2026, 9, 16).expect("a real Wednesday");
    let friday = date(2026, 9, 18).expect("a real Friday");
    let saturday = date(2026, 9, 19).expect("a real Saturday");
    let sunday = date(2026, 9, 20).expect("a real Sunday");
    let next = date(2026, 9, 21).expect("a real Monday");

    let now = at(saturday, PartOfDay::Evening);

    assert_eq!(
        state_of(
            at(monday, PartOfDay::Evening),
            Some(at(wednesday, PartOfDay::Evening)),
            now,
            &diary,
            NEITHER,
        ),
        SessionState::Skipped {
            absence: AbsenceKind::Holiday
        },
        "gym 1 fell inside the Rome holiday, and nothing had been prescribed"
    );

    assert_eq!(
        state_of(
            at(wednesday, PartOfDay::Evening),
            Some(at(friday, PartOfDay::Evening)),
            now,
            &diary,
            PERFORMED,
        ),
        SessionState::Performed,
        "cycling 1 was ridden"
    );

    assert_eq!(
        state_of(
            at(friday, PartOfDay::Evening),
            Some(at(sunday, PartOfDay::Morning)),
            now,
            &diary,
            PRESCRIBED,
        ),
        SessionState::Prescribed,
        "gym 2 went to Hevy, and a Saturday evening is still time to do it"
    );

    assert_eq!(
        state_of(
            at(sunday, PartOfDay::Morning),
            Some(at(next, PartOfDay::Evening)),
            now,
            &diary,
            NEITHER,
        ),
        SessionState::ToBePrescribed,
        "cycling 2 is the first still to be prescribed, so it is the one delivered"
    );
}

/// **An illness closes the window, and is named for closing it.**
///
/// The operator, of that same Saturday evening: *"Reporting illness will fix
/// this, because it will let the tool know that Saturday evening isn't actually
/// usable."*
#[test]
fn an_illness_over_what_is_left_makes_a_prescribed_session_not_performed() {
    let diary = september_ill_from(
        date(2026, 9, 19).expect("a real Saturday"),
        days(1).expect("one day"),
    )
    .expect("the diary builds");
    let friday = date(2026, 9, 18).expect("a real Friday");
    let saturday = date(2026, 9, 19).expect("a real Saturday");
    let sunday = date(2026, 9, 20).expect("a real Sunday");

    assert_eq!(
        state_of(
            at(friday, PartOfDay::Evening),
            Some(at(sunday, PartOfDay::Morning)),
            at(saturday, PartOfDay::Evening),
            &diary,
            PRESCRIBED,
        ),
        SessionState::NotPerformed {
            absence: Some(AbsenceKind::Illness)
        },
        "every part left in the window was taken by the illness"
    );
}

/// **A window that ran out on its own names no absence.**
///
/// The operator: run on the Sunday, the cycling slot has begun, so there is
/// *"no time left for the second gym session"*. The illness of the 17th and
/// 18th did not close this window — the Saturday was usable and went unused —
/// so nothing took it.
#[test]
fn a_window_that_simply_ran_out_names_no_absence() {
    let diary = september_ill_from(
        date(2026, 9, 17).expect("a real Thursday"),
        days(2).expect("two days"),
    )
    .expect("the diary builds");
    let friday = date(2026, 9, 18).expect("a real Friday");
    let sunday = date(2026, 9, 20).expect("a real Sunday");

    assert_eq!(
        state_of(
            at(friday, PartOfDay::Evening),
            Some(at(sunday, PartOfDay::Morning)),
            at(sunday, PartOfDay::Morning),
            &diary,
            PRESCRIBED,
        ),
        SessionState::NotPerformed { absence: None },
    );
}

/// **The tool not having been run is its own state.**
#[test]
fn a_window_that_closed_on_nothing_prescribed_is_not_prescribed() {
    let diary = september().expect("the diary builds");
    let friday = date(2026, 9, 18).expect("a real Friday");
    let sunday = date(2026, 9, 20).expect("a real Sunday");

    assert_eq!(
        state_of(
            at(friday, PartOfDay::Evening),
            Some(at(sunday, PartOfDay::Morning)),
            at(sunday, PartOfDay::Morning),
            &diary,
            NEITHER,
        ),
        SessionState::NotPrescribed,
    );
}

/// **A performance stands whatever this build wrote down.**
///
/// The operator's Wednesday ride was prescribed by a build that recorded
/// nothing: *"it was prescribed before the idea of the state machine existed"*.
/// A session the record says happened did happen, and reporting it as *not
/// prescribed* would hide a fact the store holds.
#[test]
fn a_session_performed_against_no_recorded_prescription_is_still_performed() {
    let diary = september().expect("the diary builds");
    let wednesday = date(2026, 9, 16).expect("a real Wednesday");
    let friday = date(2026, 9, 18).expect("a real Friday");

    assert_eq!(
        state_of(
            at(wednesday, PartOfDay::Evening),
            Some(at(friday, PartOfDay::Evening)),
            at(friday, PartOfDay::Evening),
            &diary,
            Recorded {
                prescribed: false,
                performed: true,
            },
        ),
        SessionState::Performed,
    );
}

/// **A part in progress counts until it ends.**
#[test]
fn the_part_a_session_is_slotted_in_is_time_it_still_has() {
    let diary = september().expect("the diary builds");
    let friday = date(2026, 9, 18).expect("a real Friday");
    let saturday = date(2026, 9, 19).expect("a real Saturday");

    assert_eq!(
        state_of(
            at(friday, PartOfDay::Evening),
            Some(at(saturday, PartOfDay::Morning)),
            at(friday, PartOfDay::Evening),
            &diary,
            PRESCRIBED,
        ),
        SessionState::Prescribed,
    );
}

/// **A holiday that keeps the ordinary week takes nothing** — #188's
/// distinction, applied to a window. Away in another zone and training at the
/// usual times is not a session lost.
#[test]
fn a_holiday_that_states_no_slots_skips_nothing() {
    let pattern = TrainingPattern::new(
        date(2026, 8, 25).expect("a real Tuesday"),
        zone("Europe/London").expect("a real zone"),
        ordinary(),
    );
    let away = Alteration::new(
        date(2026, 9, 14).expect("a real Monday"),
        days(7).expect("seven days"),
        Absence::Holiday {
            zone: Some(zone("Europe/Rome").expect("a real zone")),
            slots: None,
            reason: "away, training at the usual times".to_owned(),
        },
    );
    let diary = Diary::new(vec![pattern], vec![away]);
    let monday = date(2026, 9, 14).expect("a real Monday");
    let wednesday = date(2026, 9, 16).expect("a real Wednesday");

    assert_eq!(
        state_of(
            at(monday, PartOfDay::Evening),
            Some(at(wednesday, PartOfDay::Evening)),
            at(monday, PartOfDay::Morning),
            &diary,
            NEITHER,
        ),
        SessionState::ToBePrescribed,
        "the ordinary week stands, so the Monday is still the Monday"
    );
}

/// **A holiday that states slots takes the parts it leaves out.**
#[test]
fn a_holiday_that_states_slots_takes_the_parts_it_leaves_out() {
    let pattern = TrainingPattern::new(
        date(2026, 8, 25).expect("a real Tuesday"),
        zone("Europe/London").expect("a real zone"),
        ordinary(),
    );
    let mut kept: BTreeMap<TrainingSlot, Allocation> = BTreeMap::new();
    kept.insert(
        TrainingSlot::new(Weekday::Saturday, PartOfDay::Morning),
        Allocation::new(Discipline::Gym, harder()),
    );
    let away = Alteration::new(
        date(2026, 9, 18).expect("a real Friday"),
        days(2).expect("two days"),
        Absence::Holiday {
            zone: None,
            slots: Some(kept),
            reason: "the hotel gym is only free on the Saturday morning".to_owned(),
        },
    );
    let diary = Diary::new(vec![pattern], vec![away]);
    let friday = date(2026, 9, 18).expect("a real Friday");
    let saturday = date(2026, 9, 19).expect("a real Saturday");
    let sunday = date(2026, 9, 20).expect("a real Sunday");

    assert_eq!(
        diary.taken(at(saturday, PartOfDay::Morning)),
        None,
        "the Saturday morning is the one part the trip keeps"
    );
    assert_eq!(
        diary.taken(at(saturday, PartOfDay::Evening)),
        Some(AbsenceKind::Holiday),
    );

    assert_eq!(
        state_of(
            at(friday, PartOfDay::Evening),
            Some(at(sunday, PartOfDay::Morning)),
            at(friday, PartOfDay::Evening),
            &diary,
            PRESCRIBED,
        ),
        SessionState::Prescribed,
        "the Saturday morning is left, so Friday's session can still be done"
    );
}

/// **An open-ended window is time**, rather than a search without end.
#[test]
fn a_session_with_nothing_after_it_has_time() {
    let diary = september().expect("the diary builds");
    let sunday = date(2026, 9, 20).expect("a real Sunday");

    assert_eq!(
        state_of(
            at(sunday, PartOfDay::Morning),
            None,
            at(sunday, PartOfDay::Afternoon),
            &diary,
            NEITHER,
        ),
        SessionState::ToBePrescribed,
    );
}

/// **The hours, and the six that belong to no part of a training day.**
#[test]
fn a_part_of_the_day_knows_its_hours() {
    let hour = |at: i8| jiff::civil::Time::new(at, 0, 0, 0).expect("a real time");

    assert_eq!(PartOfDay::of(hour(5)), None, "before anyone is training");
    assert_eq!(PartOfDay::of(hour(6)), Some(PartOfDay::Morning));
    assert_eq!(PartOfDay::of(hour(11)), Some(PartOfDay::Morning));
    assert_eq!(PartOfDay::of(hour(12)), Some(PartOfDay::Afternoon));
    assert_eq!(PartOfDay::of(hour(17)), Some(PartOfDay::Afternoon));
    assert_eq!(PartOfDay::of(hour(18)), Some(PartOfDay::Evening));
    assert_eq!(PartOfDay::of(hour(23)), Some(PartOfDay::Evening));
}

/// **Before six in the morning, none of the day has been spent.**
#[test]
fn a_moment_before_the_morning_is_that_mornings() {
    let at = jiff::civil::DateTime::new(2026, 9, 20, 3, 0, 0, 0).expect("a real moment");

    assert_eq!(
        DayPart::containing(at),
        DayPart::new(
            date(2026, 9, 20).expect("a real Sunday"),
            PartOfDay::Morning
        ),
    );
}
