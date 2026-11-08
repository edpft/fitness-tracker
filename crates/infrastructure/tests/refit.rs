//! A running mesocycle cut so the term still ends in a complete mesocycle
//! (#732), on a plan shaped like the operator's autumn: SBS beside Peak 1 from
//! Monday 26 October, with the week of 2 November lost.
//!
//! At the adapter's ring because the gym's half is authored through the
//! fixtures that live here; nothing in it touches a store.

use crate::support;

use domain::{
    cycling::{
        CyclingMesocycle, CyclingMicrocycle, CyclingProvenance, CyclingSession, Interval,
        PlannedRide, PowerZone, Ride, RideVenue, SessionPosition,
    },
    macrocycle::Chain,
    measure::PositiveDuration,
    plan::{Mesocycles, Plan},
    planner::{self, Cut, Discipline, Relative, Rerun, SessionRole},
    prescription::{WeekKind, authored::Shape},
    provider::{ExternalProgramme, ProgrammeName, ProvidedFrom, Provider, PublishedAt},
    schedule::Diary,
    sequence::NonEmpty,
};
use jiff::civil::{Date, date};
use support::mesocycle;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

fn minutes(count: u64) -> Fallible<PositiveDuration> {
    Ok(PositiveDuration::from_seconds(count * 60)?)
}

/// A ride of `intervals`, `(zone, minutes)`, published at `(microcycle, session)`.
fn ride(
    at: (u32, u32),
    role: SessionRole,
    intervals: &[(PowerZone, u64)],
) -> Fallible<PlannedRide> {
    let mut parts = Vec::new();
    for (zone, length) in intervals {
        parts.push(Interval::new(*zone, minutes(*length)?));
    }
    Ok(PlannedRide::provided(
        CyclingSession::new(minutes(5)?, Ride::Intervals(NonEmpty::new(parts)?), None),
        NonEmpty::of(
            RideVenue::new(&format!("{}{}", at.0, at.1), "a class")?,
            Vec::new(),
        ),
        PublishedAt::new(at.0, at.1)?,
        role,
    ))
}

/// Four microcycles of two rides, the last a deload. **Zone 5 is ridden only
/// in microcycle 3**, so a cut that drops it stops training a zone and is
/// refused: of 2 and 3, it is 2 that goes.
fn peak_1() -> Fallible<CyclingMesocycle> {
    let harder = SessionRole::new(Relative::Higher, Relative::Lower);
    let longer = SessionRole::new(Relative::Lower, Relative::Higher);
    let harder_rides: [&[(PowerZone, u64)]; 4] = [
        &[(PowerZone::Three, 20), (PowerZone::Four, 6)],
        &[(PowerZone::Four, 17), (PowerZone::Three, 6)],
        &[
            (PowerZone::Five, 6),
            (PowerZone::Four, 12),
            (PowerZone::Three, 4),
        ],
        &[(PowerZone::Three, 10)],
    ];
    let mut weeks = Vec::new();
    for (index, intervals) in (1_u32..).zip(harder_rides) {
        let endurance = if index == 4 { 30 } else { 45 };
        let rides = [
            (
                SessionPosition::new(1)?,
                ride((index, 1), harder, intervals)?,
            ),
            (
                SessionPosition::new(2)?,
                ride((index, 2), longer, &[(PowerZone::Two, endurance)])?,
            ),
        ]
        .into_iter()
        .collect();
        weeks.push(CyclingMicrocycle::new(rides)?);
    }
    Ok(CyclingMesocycle::new(
        CyclingProvenance::Provided(ExternalProgramme::new(
            Provider::try_from("Peloton".to_owned())?,
            ProgrammeName::try_from("Peak Your Power Zones".to_owned())?,
        )),
        date(2026, 10, 26),
        NonEmpty::new(weeks)?,
    )?)
}

fn autumn() -> Fallible<Plan> {
    let sbs = mesocycle::authoring(
        mesocycle::authored(
            date(2026, 10, 26),
            Shape::Provided {
                from: ProvidedFrom::new(
                    ExternalProgramme::new(
                        Provider::try_from("Stronger By Science".to_owned())?,
                        ProgrammeName::try_from("Squat 2x Int".to_owned())?,
                    ),
                    vec![1, 2, 3, 4],
                )?,
            },
        )?,
        &[],
    )??;
    Ok(Plan::new(
        mesocycle::name("2026-autumn")?,
        jiff::Timestamp::now(),
        Some(Mesocycles::new(vec![sbs])?),
        Some(Mesocycles::new(vec![peak_1()?])?),
    )?
    .following(Chain::BuildThenPeak))
}

const LOST: Date = Date::constant(2026, 11, 2);
const AFTER: Date = Date::constant(2026, 11, 9);

const fn lost() -> [Rerun; 1] {
    [Rerun {
        monday: LOST,
        holding: None,
    }]
}

/// **The week of 2 November lost, and one week given back**: SBS drops chart
/// week 2 and Peak 1 its microcycle 2, so both end on 22 November again.
#[test]
fn a_lost_week_is_given_back_by_the_earliest_sbs_week_and_the_furthest_peak_week() {
    let plan = autumn().expect("the plan builds");
    let diary = Diary::default();
    let lengthened =
        planner::rescheduled(&plan, &lost(), &[], &diary).expect("the plan reschedules");
    assert_eq!(
        lengthened.mesocycle_on(AFTER).map(|both| both.span().end()),
        Some(date(2026, 11, 30)),
        "a re-run alone runs Peak 1 to 29 November"
    );

    let cut = planner::cutting(&lengthened, AFTER, 1).expect("a week can be cut");
    assert_eq!(
        cut,
        vec![
            Cut {
                discipline: Discipline::Gym,
                mesocycle: 0,
                dropped: vec![2],
            },
            Cut {
                discipline: Discipline::Cycling,
                mesocycle: 0,
                dropped: vec![2],
            },
        ]
    );

    let refitted = planner::rescheduled(&plan, &lost(), &cut, &diary).expect("the cut applies");
    let both = refitted.mesocycle_on(AFTER).expect("a mesocycle in force");
    assert_eq!(both.span().end(), date(2026, 11, 23));

    let gym = both.gym().expect("the gym's half");
    let charted: Vec<(Option<u32>, bool)> = [AFTER, date(2026, 11, 16)]
        .into_iter()
        .map(|monday| {
            let week = gym.calendar().place_week(monday).expect("a week of it");
            (gym.published_week(week), gym.tests_in(week))
        })
        .collect();
    assert_eq!(charted, vec![(Some(3), false), (Some(4), true)]);
    assert!(matches!(
        gym.calendar().place_week(AFTER),
        Ok(WeekKind::Climbing(at)) if at.as_u32() == 2
    ));

    let cycling = both.cycling().expect("the bike's half");
    assert_eq!(cycling.microcycle_of(AFTER), Some(2));
    assert_eq!(
        cycling
            .microcycle(2)
            .and_then(CyclingMicrocycle::published_ordinal),
        Some(3)
    );
}

/// **A cut never takes the test, nor the deload**: with week 1 ridden, two
/// weeks can go and three cannot.
#[test]
fn a_cut_keeps_the_test_and_the_deload() {
    let plan = autumn().expect("the plan builds");
    let lengthened =
        planner::rescheduled(&plan, &lost(), &[], &Diary::default()).expect("the plan reschedules");
    assert!(planner::cutting(&lengthened, AFTER, 3).is_none());
}
