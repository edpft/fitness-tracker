//! Issue #63's acceptance: the planner produces the operator's week.
//!
//! **Monday light gym, Wednesday shorter ride, Friday heavy gym, Sunday longer
//! ride**, out of the training slots and the programmes' roled sessions — and
//! in a test week the FTP test on the Wednesday, because it is the shorter and
//! harder of the two rides whatever Peloton numbers it.
//!
//! Nothing here states a weekday to a programme. The gym block and the cycling
//! mesocycle are built without one; the week comes from the diary, and the
//! planner joins the two on the role.

use std::collections::BTreeMap;

use domain::{
    cycling::{
        CyclingMesocycle, CyclingMicrocycle, CyclingProvenance, CyclingSession, Interval,
        PlannedRide, PowerZone, Ride, RideVenue, SessionPosition,
    },
    gym::exercise::{DurationExercise, Exercise, RepsExercise},
    measure::{PositiveDuration, RepCount},
    plan::{Plan, PlanName, Programme},
    planner::{self, Filled, Planned, week},
    prescription::{
        BlockPeriodisation, EntryTest, Fill, GymMesocycle, Primary, PrimaryPattern, Progression,
        Skip, SlotFills, StaticFill,
    },
    provider::{ExternalProgramme, ProgrammeName, Provider, PublishedAt},
    schedule::{
        Absence, Allocation, Alteration, Diary, Discipline, PartOfDay, Relative, SessionRole,
        TrainingPattern, TrainingSlot,
    },
    sequence::NonEmpty,
};
use jiff::{civil::Weekday, civil::date, tz::TimeZone};

type Built<T> = Result<T, Box<dyn std::error::Error>>;

const fn harder() -> SessionRole {
    SessionRole::new(Relative::Higher, Relative::Lower)
}

const fn easier() -> SessionRole {
    SessionRole::new(Relative::Lower, Relative::Higher)
}

/// The operator's week, exactly as `fitness schedule add` records it.
fn diary() -> Built<Diary> {
    let slots: BTreeMap<TrainingSlot, Allocation> = [
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
    .collect();

    Ok(Diary::new(
        vec![TrainingPattern::new(
            date(2026, 1, 1),
            domain::normalised::OperatorZone::try_from("Europe/London")?,
            slots,
        )],
        vec![],
    ))
}

fn reps(count: u32) -> Built<RepCount> {
    Ok(RepCount::new(count)?)
}

/// Every slot filled, so `SlotFills` is total.
fn fills() -> Built<SlotFills> {
    let (three, five, twenty) = (reps(3)?, reps(5)?, reps(20)?);
    let hold = |exercise| Fill::Same(Exercise::Duration(exercise));
    let lift = |exercise| Fill::Same(Exercise::Reps(exercise));
    Ok(SlotFills {
        plyometric: Fill::Same(StaticFill {
            exercise: Exercise::Reps(RepsExercise::Pogo),
            sets: three,
            reps: twenty,
        }),
        power: Fill::Same(StaticFill {
            exercise: Exercise::Reps(RepsExercise::BoxJump),
            sets: three,
            reps: five,
        }),
        knee_dominant: lift(RepsExercise::FrontSquat),
        upper_push: lift(RepsExercise::ChestDip),
        upper_pull: lift(RepsExercise::NeutralGripPullUp),
        hip_dominant: lift(RepsExercise::NordicHamstringsCurls),
        biceps: lift(RepsExercise::PreacherCurlBarbell),
        triceps: lift(RepsExercise::OverheadTricepsExtensionCable),
        wrist_flexion: lift(RepsExercise::WristFlexionDumbbell),
        wrist_extension: lift(RepsExercise::WristExtensionDumbbell),
        core: lift(RepsExercise::BentOverCableChop),
        handstand_hold: hold(DurationExercise::HandstandHold),
        dead_hang: hold(DurationExercise::DeadHang),
        hip_flexor_stretch: hold(DurationExercise::CouchStretch),
        hip_external_rotator_stretch: hold(DurationExercise::NinetyNinety),
        hamstring_stretch: hold(DurationExercise::StandingStraddleFold),
        groin_stretch: hold(DurationExercise::SquattingGroinStretch),
    })
}

/// A block of ten phase weeks from Monday 21 September.
///
/// **It states no weekday.** Its calendar is built against the diary's week,
/// which is where Monday and Friday come from.
fn gym() -> Built<GymMesocycle> {
    let diary = diary()?;
    let week = diary
        .training_week(date(2026, 9, 21), Discipline::Gym)
        .ok_or("the diary gives the gym a week")?;
    let calendar = BlockPeriodisation::weeks(
        date(2026, 9, 21),
        10,
        false,
        &[] as &[Skip],
        week,
        TimeZone::UTC,
    )?;
    Ok(GymMesocycle::Progression(Progression::BlockPeriodisation(
        BlockPeriodisation::new(
            Primary::new(
                PrimaryPattern::KneeDominant,
                Exercise::Reps(RepsExercise::FrontSquat),
                harder(),
            ),
            fills()?,
            None::<EntryTest>,
            calendar,
        )?,
    )))
}

/// One ride: a warm-up, one zone held, and a name.
fn ride(
    seconds: u64,
    called: &str,
    published: PublishedAt,
    role: SessionRole,
) -> Built<PlannedRide> {
    let session = CyclingSession::new(
        PositiveDuration::from_seconds(600)?,
        Ride::Intervals(NonEmpty::new(vec![Interval::new(
            PowerZone::Three,
            PositiveDuration::from_seconds(seconds)?,
        )])?),
        None,
    );
    Ok(PlannedRide::provided(
        session,
        NonEmpty::of(
            RideVenue::new("0bc8a790d8ca49cc8355cc7411842ca9", called)?,
            Vec::new(),
        ),
        published,
        role,
    ))
}

/// Four microcycles from Monday 21 September.
///
/// **A test week rides the same two roles as any other week**; Peloton just
/// files the FTP test as the microcycle's third session, which is this
/// mesocycle's second. It is the shorter and harder of the two, so it carries
/// the harder role and takes the Wednesday exactly as every week's harder
/// session does.
fn cycling(test: bool) -> Built<CyclingMesocycle> {
    let mut weeks = Vec::with_capacity(4);
    for ordinal in 1..=4_u32 {
        let at = |session| PublishedAt::new(ordinal, session);
        let (first, second) = if test {
            (
                ride(2700, "45 min Power Zone Endurance Ride", at(1)?, easier())?,
                ride(1200, "20 min FTP Test Ride", at(3)?, harder())?,
            )
        } else {
            (
                ride(1800, "45 min Power Zone Ride", at(1)?, harder())?,
                ride(2400, "60 min Power Zone Endurance Ride", at(3)?, easier())?,
            )
        };
        weeks.push(CyclingMicrocycle::new(
            [
                (SessionPosition::new(1)?, first),
                (SessionPosition::new(2)?, second),
            ]
            .into_iter()
            .collect(),
        )?);
    }

    Ok(CyclingMesocycle::new(
        CyclingProvenance::Provided(ExternalProgramme::new(
            Provider::try_from("Peloton".to_owned())?,
            ProgrammeName::try_from("Build Your Power Zones".to_owned())?,
        )),
        date(2026, 9, 21),
        NonEmpty::new(weeks)?,
    )?)
}

fn plan(test: bool) -> Built<Plan> {
    Ok(Plan::new(
        PlanName::try_from("autumn".to_owned())?,
        jiff::Timestamp::UNIX_EPOCH,
        Some(Programme::new(vec![gym()?])?),
        Some(Programme::new(vec![cycling(test)?])?),
    )?)
}

/// How a planned slot reads, for an assertion that is a week rather than a
/// field-by-field walk.
fn line(planned: &Planned<'_>) -> String {
    let what = match &planned.session {
        Ok(Filled::Gym { week, .. }) => format!("gym, {week}"),
        Ok(Filled::Cycling { ride, .. }) => ride.at().first().called().to_owned(),
        Err(unfilled) => format!("nothing: {unfilled}"),
    };
    format!(
        "{:?} {} — {what}",
        planned.slot.date.weekday(),
        planned.slot.role
    )
}

/// **The operator's week, out of the slots and the programmes' roled
/// sessions.** Issue #63's "done when", first half.
#[test]
fn the_planner_produces_the_operators_week() {
    let (Ok(plan), Ok(diary)) = (plan(false), diary()) else {
        panic!("the fixture plan and diary are valid")
    };

    // Week two of the block: Monday 28 September to Sunday 4 October.
    let planned = week(&plan, &diary, date(2026, 9, 30));
    let read: Vec<String> = planned.iter().map(line).collect();

    assert_eq!(
        read,
        vec![
            "Monday lower intensity, higher volume — gym, week 2",
            "Wednesday higher intensity, lower volume — 45 min Power Zone Ride",
            "Friday higher intensity, lower volume — gym, week 2",
            "Sunday lower intensity, higher volume — 60 min Power Zone Endurance Ride",
        ]
    );
}

/// **And in a test week the FTP test is on the Wednesday.** Issue #63's "done
/// when", second half, and the case that proves a published order does not
/// place a session: the test is the mesocycle's *second* ride and takes the
/// week's *first* cycling slot.
#[test]
fn a_test_week_puts_the_ftp_test_on_the_wednesday() {
    let (Ok(plan), Ok(diary)) = (plan(true), diary()) else {
        panic!("the fixture plan and diary are valid")
    };

    let planned = week(&plan, &diary, date(2026, 9, 23));
    let read: Vec<String> = planned.iter().map(line).collect();

    assert_eq!(
        read,
        vec![
            "Monday lower intensity, higher volume — gym, week 1",
            "Wednesday higher intensity, lower volume — 20 min FTP Test Ride",
            "Friday higher intensity, lower volume — gym, week 1",
            "Sunday lower intensity, higher volume — 45 min Power Zone Endurance Ride",
        ]
    );

    let Some(wednesday) = planned.get(1) else {
        panic!("the week holds four slots")
    };
    let Ok(Filled::Cycling { position, .. }) = &wednesday.session else {
        panic!("the Wednesday rides")
    };
    assert_eq!(
        position.as_u8(),
        2,
        "the test is the week's second session and the Wednesday is still its slot"
    );
}

/// A week before the plan starts holds its slots and nothing to put in them.
///
/// **A real state, not a fault.** The operator's week does not stop existing
/// because no block covers it, and saying what is missing is more use than
/// leaving the days out.
#[test]
fn a_week_no_mesocycle_covers_is_reported_as_unfilled() {
    let (Ok(plan), Ok(diary)) = (plan(false), diary()) else {
        panic!("the fixture plan and diary are valid")
    };

    let planned = week(&plan, &diary, date(2026, 6, 1));
    assert_eq!(planned.len(), 4, "the week still holds four slots");
    assert!(
        planned.iter().all(|slot| slot.session.is_err()),
        "and nothing in the plan reaches June"
    );
}

/// **An absence does not delete a session from the week** (#185).
///
/// The operator's own week of 14 September: away in Rome to the Monday, ill on
/// the Thursday and Friday. Until 2026-09-20 this read the *altered* diary, so
/// the two days an absence covered had no slots and the week came back as two
/// sessions rather than four — silently missing the gym test he was too ill to
/// do. A session that could not be trained is still a session of the
/// microcycle; why it did not happen is a state, not an omission.
#[test]
fn a_week_keeps_the_sessions_an_absence_covered() {
    let Ok(plan) = plan(true) else {
        panic!("the fixture plan is valid")
    };
    let Ok(diary) = diary() else {
        panic!("the fixture diary is valid")
    };
    let Some(over) = std::num::NonZeroU8::new(2) else {
        panic!("two is not zero")
    };

    let ill = Alteration::new(date(2026, 9, 17), over, Absence::Illness);
    let diary = Diary::new(diary.patterns().to_vec(), vec![ill]);

    let planned = week(&plan, &diary, date(2026, 9, 16));
    let read: Vec<String> = planned
        .iter()
        .map(|one| format!("{} {}", one.slot.discipline, one.number))
        .collect();

    assert_eq!(
        read,
        vec!["gym 1", "cycling 1", "gym 2", "cycling 2"],
        "four sessions, numbered per discipline in the order the week runs"
    );
}

/// One day's absence, of a stated kind.
///
/// Fallible and unwrapped at the call site, as the fixtures above are.
fn absent(on: jiff::civil::Date, absence: Absence) -> Built<Diary> {
    let Some(one) = std::num::NonZeroU8::new(1) else {
        return Err("one is not zero".into());
    };
    Ok(Diary::new(
        diary()?.patterns().to_vec(),
        vec![Alteration::new(on, one, absence)],
    ))
}

/// A holiday that keeps the ordinary week, so only its kind differs from an
/// illness.
fn holiday() -> Absence {
    Absence::FamilyHoliday {
        zone: None,
        slots: Some(BTreeMap::new()),
        reason: "Rome".to_owned(),
    }
}

/// **An illness eases what survives it, whichever slot that is.**
///
/// The operator, 2026-09-20: *"if one session was lost to illness, the
/// remaining session would be easier, so by definition, there can't be two
/// sessions after illness in a microcycle."* The Sunday is lost, so the
/// surviving Wednesday rides the lower-intensity session rather than the
/// higher-intensity one its slot ordinarily asks for.
#[test]
fn illness_in_the_week_eases_the_session_that_survives_it() {
    let Ok(diary) = absent(date(2026, 9, 27), Absence::Illness) else {
        panic!("the fixture diary is valid")
    };

    let eased = planner::eased_by_illness(&diary, date(2026, 9, 23), Discipline::Cycling, harder());
    assert_eq!(
        eased,
        easier(),
        "the Sunday was lost to illness, so the Wednesday rides the easier session"
    );
}

/// **A holiday does not ease anything.** Time away says nothing about fitness;
/// illness is assumed to have cost some, which is the whole reason an absence
/// records its kind (#178).
#[test]
fn a_holiday_leaves_the_surviving_session_alone() {
    let Ok(diary) = absent(date(2026, 9, 27), holiday()) else {
        panic!("the fixture diary is valid")
    };

    let kept = planner::eased_by_illness(&diary, date(2026, 9, 23), Discipline::Cycling, harder());
    assert_eq!(kept, harder(), "a holiday is not an illness");
}

/// **The slot being asked about is not one of the ones that could be lost.**
/// A session prescribed for a day is not a session missed on it, whatever the
/// diary says about the rest of that day — otherwise every slot would ease
/// itself and the harder ride would never be prescribed at all.
#[test]
fn a_slot_is_not_eased_by_an_illness_on_its_own_day() {
    let Ok(diary) = absent(date(2026, 9, 23), Absence::Illness) else {
        panic!("the fixture diary is valid")
    };

    let kept = planner::eased_by_illness(&diary, date(2026, 9, 23), Discipline::Cycling, harder());
    assert_eq!(kept, harder(), "this slot's own day does not ease it");
}

/// **A gym session lost to illness does not ease a ride.** The two disciplines
/// are held apart on purpose: what happens when one waits for the other is the
/// plan's business (#177), not this slot's.
#[test]
fn an_illness_in_another_discipline_does_not_ease_this_one() {
    let Ok(diary) = absent(date(2026, 9, 25), Absence::Illness) else {
        panic!("the fixture diary is valid")
    };

    let kept = planner::eased_by_illness(&diary, date(2026, 9, 23), Discipline::Cycling, harder());
    assert_eq!(
        kept,
        harder(),
        "the Friday is the gym's slot, and the bike does not read it"
    );
}
