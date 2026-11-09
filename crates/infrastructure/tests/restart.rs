//! A lost progressive week starts the programme again (#732), on a plan shaped
//! like the operator's autumn: SBS beside Peak 1 from Monday 26 October, with
//! the week of 2 November lost.
//!
//! At the adapter's ring because the gym's half is authored through the
//! fixtures that live here; nothing in it touches a store.

use crate::support;

use application::PlanStore as _;
use domain::{
    cycling::{
        CyclingMesocycle, CyclingMicrocycle, CyclingProvenance, CyclingSession, Interval,
        PlannedRide, PowerZone, Ride, RideVenue, SessionPosition,
    },
    macrocycle::Chain,
    measure::PositiveDuration,
    plan::{Mesocycles, Plan},
    planner::{self, Discipline, Placed, Relative, Restart, SessionRole, SessionState},
    prescription::authored::Shape,
    provider::{ExternalProgramme, ProgrammeName, ProvidedFrom, Provider, PublishedAt},
    schedule::Diary,
    sequence::NonEmpty,
};
use jiff::civil::{Date, date};
use support::{corpus, mesocycle};

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

/// Four microcycles of two rides from `start`, the last a deload.
fn peak_1(start: Date) -> Fallible<CyclingMesocycle> {
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
        start,
        NonEmpty::new(weeks)?,
    )?)
}

/// An SBS cycle of the chart's four weeks from `start`.
fn sbs(start: Date) -> Fallible<domain::prescription::GymMesocycle> {
    Ok(mesocycle::authoring(
        mesocycle::authored(
            start,
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
    )??)
}

fn autumn() -> Fallible<Plan> {
    let start = date(2026, 10, 26);
    Ok(Plan::new(
        mesocycle::name("2026-autumn")?,
        jiff::Timestamp::now(),
        Some(Mesocycles::new(vec![sbs(start)?])?),
        Some(Mesocycles::new(vec![peak_1(start)?])?),
    )?
    .following(Chain::BuildThenPeak))
}

const RESTARTED: Date = Date::constant(2026, 11, 9);

/// **The mesocycle in force ends the Sunday before the restart**: two weeks
/// of each, the one ridden and the one lost, and nothing from 9 November until
/// the boundary commits what is left of the term.
#[test]
fn a_restart_ends_the_mesocycle_in_force_the_sunday_before() {
    let plan = autumn().expect("the plan builds");
    let restarted = planner::rescheduled(
        &plan,
        &[],
        &[Restart { monday: RESTARTED }],
        &Diary::default(),
    )
    .expect("the plan reschedules");

    assert_eq!(restarted.span().end(), RESTARTED);
    assert!(restarted.mesocycle_on(RESTARTED).is_none());
    let both = restarted
        .mesocycle_on(date(2026, 11, 2))
        .expect("the mesocycle in force");
    let gym = both.gym().expect("the gym's half");
    let charted: Vec<Option<u32>> = [date(2026, 10, 26), date(2026, 11, 2)]
        .into_iter()
        .map(|monday| {
            let week = gym.calendar().place_week(monday).expect("a week of it");
            gym.published_week(week)
        })
        .collect();
    assert_eq!(charted, vec![Some(1), Some(2)]);
    let cycling = both.cycling().expect("the bike's half");
    assert_eq!(cycling.duration_weeks(), 2);
}

/// **A restart on a mesocycle's first Monday ends nothing**: it is where a
/// committed one starts, and it stands.
#[test]
fn a_restart_on_a_mesocycles_first_monday_leaves_it_whole() {
    let plan = autumn().expect("the plan builds");
    let start = plan.span().start();
    let restarted =
        planner::rescheduled(&plan, &[], &[Restart { monday: start }], &Diary::default())
            .expect("the plan reschedules");
    assert_eq!(restarted.span(), plan.span());
}

const fn placed(discipline: Discipline, test: bool, state: SessionState) -> Placed {
    Placed {
        discipline,
        role: planner::ESSENTIAL,
        state,
        test,
    }
}

/// **A lost progressive week starts again; a lost test runs again**, since a
/// test anchors what follows.
#[test]
fn only_a_lost_progressive_week_starts_again() {
    let missed = SessionState::NotPerformed { absence: None };
    let progressive = [
        placed(Discipline::Gym, false, SessionState::Performed),
        placed(Discipline::Cycling, false, missed.clone()),
    ];
    assert!(planner::progressive(&progressive, &[]));

    let testing = [
        placed(Discipline::Gym, true, SessionState::Performed),
        placed(Discipline::Cycling, true, missed),
    ];
    assert!(!planner::progressive(&testing, &[]));
}

/// **The store ends the mesocycle in force in place** when what follows the
/// restart is committed, so the plan reads back with nothing on one day twice
/// and the ended mesocycles keep their identity.
#[test]
fn committing_after_a_restart_ends_the_stored_mesocycle_in_place() {
    let read = corpus::block_on(async {
        let directory = tempfile::tempdir()?;
        let pool = infrastructure::connect(&directory.path().join("test.db")).await?;
        mesocycle::record_the_week(&pool).await?;
        let plans = infrastructure::SqlitePlanStore::new(pool, corpus::zone()?);
        let authored = autumn()?;
        plans.author(&authored).await?;

        let restarted = planner::rescheduled(
            &authored,
            &[],
            &[Restart { monday: RESTARTED }],
            &Diary::default(),
        )?;
        let gym = restarted.gym().and_then(|side| side.mesocycles().last());
        let cycling = restarted
            .cycling()
            .and_then(|side| side.mesocycles().last());
        let ending = gym
            .zip(cycling)
            .map(|(gym, cycling)| application::Ending { gym, cycling });
        plans
            .commit(
                authored.name(),
                ending,
                &[sbs(RESTARTED)?],
                &[peak_1(RESTARTED)?],
            )
            .await?;
        let read: Option<Plan> = plans.named(authored.name()).await?;
        Ok::<_, Box<dyn std::error::Error>>(read)
    })
    .expect("a runtime")
    .expect("the store writes and reads")
    .expect("the plan is there");

    let gym: Vec<(Date, u32)> = read
        .gym()
        .expect("a gym side")
        .mesocycles()
        .map(|mesocycle| {
            (
                mesocycle.calendar().start(),
                mesocycle.calendar().duration_weeks(),
            )
        })
        .collect();
    assert_eq!(gym, vec![(date(2026, 10, 26), 2), (RESTARTED, 4)]);
    let cycling: Vec<(Date, usize)> = read
        .cycling()
        .expect("a cycling side")
        .mesocycles()
        .map(|mesocycle| (mesocycle.start(), mesocycle.duration_weeks()))
        .collect();
    assert_eq!(cycling, vec![(date(2026, 10, 26), 2), (RESTARTED, 4)]);
}
