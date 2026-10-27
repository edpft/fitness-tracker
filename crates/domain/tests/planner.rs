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
    plan::{Mesocycles, Plan, PlanName},
    planner::{self, Filled, Missed, Planned, week},
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
        knee_dominant: lift(RepsExercise::FrontSquatBarbell),
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
                Exercise::Reps(RepsExercise::FrontSquatBarbell),
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
        Some(Mesocycles::new(vec![gym()?])?),
        Some(Mesocycles::new(vec![cycling(test)?])?),
    )?)
}

/// How a planned slot reads, for an assertion that is a week rather than a
/// field-by-field walk.
fn line(planned: &Planned<'_>) -> String {
    let what = match &planned.session {
        Ok(Filled::Gym { week, .. }) => format!("gym, {week}"),
        Ok(Filled::Cycling { ride, .. }) => ride.at().first().called().to_owned(),
        Ok(Filled::CyclingHolding { .. }) => "a holding ride".to_owned(),
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
    let planned = week(&plan, &diary, date(2026, 9, 30), &Missed::none());
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

    let planned = week(&plan, &diary, date(2026, 9, 23), &Missed::none());
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

    let planned = week(&plan, &diary, date(2026, 6, 1), &Missed::none());
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

    let planned = week(&plan, &diary, date(2026, 9, 16), &Missed::none());
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

/// The diary with one skip, from `from` up to `until`.
fn skipping(
    diary: Diary,
    from: (jiff::civil::Date, PartOfDay),
    until: (jiff::civil::Date, PartOfDay),
) -> Built<Diary> {
    let skip = domain::schedule::Skip::new(
        domain::schedule::DayPart::new(from.0, from.1),
        domain::schedule::DayPart::new(until.0, until.1),
        "work dinner",
    )?;
    Ok(diary.with_skips(vec![skip]))
}

/// The role each slot of a discipline rides in the week of Monday 21 September.
fn roles(diary: &Diary, discipline: Discipline) -> Vec<(jiff::civil::Date, SessionRole)> {
    roles_missing(diary, discipline, &Missed::none())
}

/// The same, with the `missed` essential sessions moved.
fn roles_missing(
    diary: &Diary,
    discipline: Discipline,
    missed: &Missed,
) -> Vec<(jiff::civil::Date, SessionRole)> {
    planner::slots(diary, date(2026, 9, 21), missed)
        .into_iter()
        .filter(|slot| slot.discipline == discipline)
        .map(|slot| (slot.date, slot.role))
        .collect()
}

/// **A skipped essential ride takes the Sunday** (#392). Skipping the Wednesday
/// evening runs to the Friday evening gym slot, which leaves the Sunday's slot
/// free, so the harder ride moves there and the easier one is what the week
/// loses.
#[test]
fn a_skipped_essential_session_takes_the_next_slot_of_its_discipline() {
    let Ok(diary) = diary().and_then(|diary| {
        skipping(
            diary,
            (date(2026, 9, 23), PartOfDay::Evening),
            (date(2026, 9, 25), PartOfDay::Evening),
        )
    }) else {
        panic!("the fixture diary is valid")
    };

    assert_eq!(
        roles(&diary, Discipline::Cycling),
        vec![(date(2026, 9, 23), easier()), (date(2026, 9, 27), harder())],
    );
    assert_eq!(
        planner::role_on(
            &diary,
            date(2026, 9, 27),
            Discipline::Cycling,
            &Missed::none()
        ),
        Some(harder()),
    );
    assert_eq!(
        roles(&diary, Discipline::Gym),
        vec![(date(2026, 9, 21), easier()), (date(2026, 9, 25), harder())],
        "the gym's week is not touched by a ride's skip"
    );
}

/// **No later slot of the discipline, nothing moves.** The Friday is the gym's
/// last slot, so a skipped Friday leaves the essential session where it was,
/// skipped, and the microcycle re-runs.
#[test]
fn a_skipped_essential_session_with_no_later_slot_stays_put() {
    let Ok(diary) = diary().and_then(|diary| {
        skipping(
            diary,
            (date(2026, 9, 25), PartOfDay::Evening),
            (date(2026, 9, 27), PartOfDay::Morning),
        )
    }) else {
        panic!("the fixture diary is valid")
    };

    assert_eq!(
        roles(&diary, Discipline::Gym),
        vec![(date(2026, 9, 21), easier()), (date(2026, 9, 25), harder())],
    );
}

/// **A skipped session that is not essential moves nothing.**
#[test]
fn a_skipped_session_that_is_not_essential_moves_nothing() {
    let Ok(ordinary) = diary() else {
        panic!("the fixture diary is valid")
    };
    let Ok(skipped) = skipping(
        ordinary.clone(),
        (date(2026, 9, 21), PartOfDay::Afternoon),
        (date(2026, 9, 23), PartOfDay::Evening),
    ) else {
        panic!("the skip is valid")
    };

    assert_eq!(
        planner::slots(&skipped, date(2026, 9, 21), &Missed::none()),
        planner::slots(&ordinary, date(2026, 9, 21), &Missed::none()),
    );
}

/// **A later slot already taken is not left.** With the Sunday lost to a
/// holiday, a skipped Wednesday has nowhere to go.
#[test]
fn a_later_slot_taken_by_an_absence_does_not_take_the_essential_session() {
    let Ok(diary) = absent(date(2026, 9, 27), holiday()).and_then(|diary| {
        skipping(
            diary,
            (date(2026, 9, 23), PartOfDay::Evening),
            (date(2026, 9, 25), PartOfDay::Evening),
        )
    }) else {
        panic!("the fixture diary is valid")
    };

    assert_eq!(
        roles(&diary, Discipline::Cycling),
        vec![(date(2026, 9, 23), harder()), (date(2026, 9, 27), easier())],
    );
}

/// **A missed essential ride takes the Sunday too** (2026-10-22). The
/// Wednesday's window closed with the harder ride not ridden and no skip, so
/// the Sunday rides it, as it would have after a skip.
#[test]
fn a_missed_essential_session_takes_the_next_slot_of_its_discipline() {
    let Ok(diary) = diary() else {
        panic!("the fixture diary is valid")
    };
    let missed: Missed = std::iter::once((Discipline::Cycling, date(2026, 9, 23))).collect();

    assert_eq!(
        roles_missing(&diary, Discipline::Cycling, &missed),
        vec![(date(2026, 9, 23), easier()), (date(2026, 9, 27), harder())],
    );
    assert_eq!(
        planner::role_on(&diary, date(2026, 9, 27), Discipline::Cycling, &missed),
        Some(harder()),
    );
    assert_eq!(
        roles_missing(&diary, Discipline::Gym, &missed),
        vec![(date(2026, 9, 21), easier()), (date(2026, 9, 25), harder())],
        "the gym's week is not touched by a missed ride"
    );
}

/// **No later slot of the discipline, nothing moves**, missed as skipped: the
/// Friday is the gym's last slot.
#[test]
fn a_missed_essential_session_with_no_later_slot_stays_put() {
    let Ok(diary) = diary() else {
        panic!("the fixture diary is valid")
    };
    let missed: Missed = std::iter::once((Discipline::Gym, date(2026, 9, 25))).collect();

    assert_eq!(
        roles_missing(&diary, Discipline::Gym, &missed),
        vec![(date(2026, 9, 21), easier()), (date(2026, 9, 25), harder())],
    );
}

/// **Only the essential session moves.** The Sunday's ride is the easier one,
/// so missing it leaves the week as the diary holds it.
#[test]
fn a_missed_session_that_is_not_essential_moves_nothing() {
    let Ok(diary) = diary() else {
        panic!("the fixture diary is valid")
    };
    let missed: Missed = std::iter::once((Discipline::Cycling, date(2026, 9, 27))).collect();

    assert_eq!(
        planner::slots(&diary, date(2026, 9, 21), &missed),
        planner::slots(&diary, date(2026, 9, 21), &Missed::none()),
    );
}

/// **The rule reads slots, not weekdays.** A week with the heavy gym session
/// on the Tuesday and two lighter ones after it: skipping the Tuesday puts the
/// heavy session in the Thursday, the next gym slot, and leaves the Saturday as
/// it was.
#[test]
fn the_next_slot_takes_it_whatever_day_it_is() {
    let slots: BTreeMap<TrainingSlot, Allocation> = [
        (Weekday::Tuesday, harder()),
        (Weekday::Thursday, easier()),
        (Weekday::Saturday, easier()),
    ]
    .into_iter()
    .map(|(day, role)| {
        (
            TrainingSlot::new(day, PartOfDay::Evening),
            Allocation::new(Discipline::Gym, role),
        )
    })
    .collect();
    let Ok(zone) = domain::normalised::OperatorZone::try_from("Europe/London") else {
        panic!("a real zone")
    };
    let week = Diary::new(
        vec![TrainingPattern::new(date(2026, 1, 1), zone, slots)],
        vec![],
    );
    let Ok(diary) = skipping(
        week,
        (date(2026, 9, 22), PartOfDay::Evening),
        (date(2026, 9, 24), PartOfDay::Evening),
    ) else {
        panic!("the skip is valid")
    };

    assert_eq!(
        roles(&diary, Discipline::Gym),
        vec![
            (date(2026, 9, 22), easier()),
            (date(2026, 9, 24), harder()),
            (date(2026, 9, 26), easier()),
        ],
    );
}
