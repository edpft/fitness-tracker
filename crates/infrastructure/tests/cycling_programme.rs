//! The authored cycling programme, through its real store (§ 12).
//!
//! **The point of storing the zone plan is that it comes back exactly.** A ride
//! read from Peloton in September is prescribed in December from these rows and
//! from nothing else — no re-fetch, no network, and § 13's reproducibility is
//! whatever this round trip preserves. So the assertion is equality of the whole
//! programme rather than of a summary of it.

mod support;

use application::{CyclingMesocycleStore as _, PlanAuthor as _, PlanStore as _};
use domain::{
    cycling::{
        CyclingMesocycle, CyclingMicrocycle, CyclingSession, CyclingWeekdays, Interval,
        PlannedRide, PowerZone, Ride, RideVenue, SessionPosition,
    },
    gym::{PositiveDuration, sequence::NonEmpty},
    plan::{Plan, PlanName, Programme},
    provider::{ExternalProgramme, ProgrammeName, Provider},
};
use infrastructure::{
    SqliteCyclingMesocycleStore, SqliteGenerationParameterStore, SqliteGymMesocycleStore,
    SqlitePlanStore, connect,
};
use jiff::civil::{Date, Weekday, date};
use support::{corpus, programme as gym};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// The three stores one plan is written and read through.
struct Opened {
    plans: SqlitePlanStore,
    cycling: SqliteCyclingMesocycleStore,
    gym: SqliteGymMesocycleStore,
    parameters: SqliteGenerationParameterStore,
}

async fn store() -> Fallible<(Opened, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    Ok((
        Opened {
            plans: SqlitePlanStore::new(pool.clone(), corpus::zone()?),
            cycling: SqliteCyclingMesocycleStore::new(pool.clone()),
            gym: SqliteGymMesocycleStore::new(pool.clone(), corpus::zone()?),
            parameters: SqliteGenerationParameterStore::new(pool),
        },
        directory,
    ))
}

/// A plan whose cycling programme is the mesocycles handed in.
fn plan(called: &str, mesocycles: Vec<CyclingMesocycle>) -> Fallible<Plan> {
    Ok(Plan::new(
        PlanName::try_from(called.to_owned())?,
        jiff::Timestamp::now(),
        None,
        Some(Programme::new(mesocycles)?),
    )?)
}

/// Author a plan through the use case, so the overlap rule runs.
async fn author(opened: &Opened, plan: &Plan) -> Fallible<application::Authored> {
    let (_, authored) = application::prescribe::Authoring::new(
        opened.plans.clone(),
        opened.gym.clone(),
        opened.parameters.clone(),
    )
    .author(plan, &gym::parameters()?)
    .await?;
    Ok(authored)
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
            Ok(Err(error)) => panic!("the operation succeeds: {error}"),
            Err(error) => panic!("a runtime is available: {error}"),
        }
    };
}

/// A ride of several zones in order, at one class.
fn intervals(reference: &str, called: &str, published: u32) -> Fallible<PlannedRide> {
    let runs = [
        (PowerZone::One, 300),
        (PowerZone::Three, 600),
        (PowerZone::Five, 180),
        (PowerZone::Three, 600),
        (PowerZone::Two, 420),
    ];
    let runs = runs
        .into_iter()
        .map(|(zone, seconds)| {
            Ok(Interval::new(
                zone,
                PositiveDuration::from_seconds(seconds)?,
            ))
        })
        .collect::<Fallible<Vec<_>>>()?;

    let session = CyclingSession::new(
        PositiveDuration::from_seconds(780)?,
        Ride::Intervals(NonEmpty::new(runs)?),
        Some(PositiveDuration::from_seconds(60)?),
    );
    Ok(PlannedRide::new(
        session,
        NonEmpty::of(RideVenue::new(reference, called)?, Vec::new()),
        published,
    ))
}

/// The FTP warm-up and the test itself: one session, two places, no cool-down.
fn ftp_test() -> Fallible<PlannedRide> {
    let session = CyclingSession::new(
        PositiveDuration::from_seconds(600)?,
        Ride::Effort(PositiveDuration::from_seconds(1200)?),
        None,
    );
    Ok(PlannedRide::new(
        session,
        NonEmpty::of(
            RideVenue::new("1eabf70b20744f48b99259f93889ced5", "10 min FTP Warmup Ride")?,
            vec![RideVenue::new(
                "4d302bef49574118a071269bed38bd30",
                "20 min FTP Test Ride",
            )?],
        ),
        3,
    ))
}

fn microcycle(published: u32, test: bool) -> Fallible<CyclingMicrocycle> {
    let second = if test {
        ftp_test()?
    } else {
        intervals(
            "414a518108ea4c5cada00ab9899a9d8d",
            "60 min Power Zone Ride",
            3,
        )?
    };
    let rides = [
        (
            SessionPosition::new(1)?,
            intervals(
                "9f8f3af689cc4f0db9afa013d4676ed6",
                "45 min Power Zone Endurance Ride",
                1,
            )?,
        ),
        // The operator's second ride of the week, taken from the published
        // third — numbered as his, with the published number kept beside it.
        (SessionPosition::new(2)?, second),
    ];
    Ok(CyclingMicrocycle::new(
        rides.into_iter().collect(),
        published,
    )?)
}

fn programme(start: Date, published: &[u32], test: bool) -> Fallible<CyclingMesocycle> {
    let weeks = published
        .iter()
        .map(|number| microcycle(*number, test))
        .collect::<Fallible<Vec<_>>>()?;
    let weekdays = CyclingWeekdays::new(vec![
        (Weekday::Wednesday, SessionPosition::new(1)?),
        (Weekday::Sunday, SessionPosition::new(2)?),
    ])?;
    Ok(CyclingMesocycle::new(
        ExternalProgramme::new(
            Provider::try_from("Peloton".to_owned())?,
            ProgrammeName::try_from("Build Your Power Zones".to_owned())?,
        ),
        start,
        NonEmpty::new(weeks)?,
        weekdays,
    )?)
}

/// Every interval, every venue and the order of both, exactly as authored.
#[test]
fn an_authored_mesocycle_round_trips_exactly() {
    let (opened, _directory) = opened!();
    let Ok(authored) = programme(date(2026, 9, 21), &[1, 2, 4, 5], false) else {
        panic!("the fixture mesocycle is valid")
    };
    let Ok(plan) = plan("autumn", vec![authored.clone()]) else {
        panic!("the fixture plan is valid")
    };

    run!(author(&opened, &plan));

    let Some((_, name, read_back)) = run!(opened.cycling.on(date(2026, 9, 23))) else {
        panic!("the mesocycle covers the date it was authored for")
    };
    assert_eq!(name.as_str(), "autumn", "and it knows the plan it is in");
    assert_eq!(read_back, authored, "the whole mesocycle, not a summary");
}

/// A ride with no zones and no cool-down is the FTP test, and absent must not
/// come back as zero: they are different claims and only one is true.
#[test]
fn an_effort_round_trips_with_no_cool_down() {
    let (opened, _directory) = opened!();
    let Ok(authored) = programme(date(2026, 9, 14), &[5], true) else {
        panic!("the fixture mesocycle is valid")
    };
    let Ok(plan) = plan("autumn", vec![authored]) else {
        panic!("the fixture plan is valid")
    };

    run!(author(&opened, &plan));

    let Some((_, _, read_back)) = run!(opened.cycling.on(date(2026, 9, 20))) else {
        panic!("the test week covers its Sunday")
    };
    let Some((_, _, ride)) = read_back.on(date(2026, 9, 20)) else {
        panic!("Sunday rides the test")
    };
    assert_eq!(ride.session().cool_down(), None, "absent, not zero");
    assert!(
        matches!(ride.session().ride(), Ride::Effort(duration) if duration.as_seconds() == 1200),
        "twenty minutes and no zone",
    );
    assert_eq!(
        ride.at().count(),
        2,
        "the warm-up and the test are one session"
    );
}

/// Versions of one plan sit on top of one another; the latest answers.
#[test]
fn re_authoring_a_name_supersedes_rather_than_competing() {
    let (opened, _directory) = opened!();
    let (Ok(first), Ok(corrected)) = (
        programme(date(2026, 9, 21), &[1, 2, 4, 5], false),
        programme(date(2026, 9, 21), &[1, 2, 3, 4], false),
    ) else {
        panic!("both fixtures are valid")
    };
    let (Ok(before), Ok(after)) = (
        plan("autumn", vec![first]),
        plan("autumn", vec![corrected.clone()]),
    ) else {
        panic!("the fixture plans are valid")
    };

    let created = run!(author(&opened, &before));
    let modified = run!(author(&opened, &after));

    assert_eq!(created, application::Authored::Created);
    assert_eq!(
        modified,
        application::Authored::Modified,
        "the same name is a re-authoring, not a rival",
    );

    let Some((_, _, read_back)) = run!(opened.cycling.on(date(2026, 9, 23))) else {
        panic!("the mesocycle covers the date")
    };
    assert_eq!(read_back, corrected, "the latest authoring answers");
}

/// Two *plans* covering one day would make which of them answers depend on the
/// order rows came back in.
///
/// **Two mesocycles of one plan are a different rule**, and `Programme::new`
/// refuses those before a store is involved at all. What this asserts is the
/// rule that survived #86: the gym and the bike inside one plan compete for
/// every day on purpose, and two plans may not.
#[test]
fn a_second_plan_covering_the_same_days_is_refused() {
    let (opened, _directory) = opened!();
    let (Ok(first), Ok(overlapping)) = (
        programme(date(2026, 9, 21), &[1, 2, 4, 5], false),
        programme(date(2026, 10, 12), &[1, 2, 4, 5], false),
    ) else {
        panic!("both fixtures are valid")
    };
    let (Ok(autumn), Ok(winter)) = (
        plan("autumn", vec![first]),
        plan("winter", vec![overlapping]),
    ) else {
        panic!("the fixture plans are valid")
    };

    run!(author(&opened, &autumn));

    let Ok(refused) = corpus::block_on(author(&opened, &winter)) else {
        panic!("a runtime is available")
    };
    let Err(error) = refused else {
        panic!("one day, two plans")
    };
    assert!(
        error
            .to_string()
            .contains("two plans may not answer for one day"),
        "got {error}",
    );
}

/// A plan's cycling programme runs four mesocycles back to back, so a question
/// asked after the last ride of one has its answer in the next.
#[test]
fn the_next_ride_crosses_a_mesocycle_boundary() {
    let (opened, _directory) = opened!();
    let (Ok(first), Ok(second)) = (
        programme(date(2026, 9, 21), &[1, 2, 4, 5], false),
        programme(date(2026, 10, 19), &[1, 2, 4, 5], false),
    ) else {
        panic!("both fixtures are valid")
    };
    let Ok(autumn) = plan("autumn", vec![first, second]) else {
        panic!("the fixture plan is valid")
    };

    run!(author(&opened, &autumn));

    // Monday 2026-10-19 opens the second mesocycle; asked from the Monday
    // after the first one's last Sunday, the answer is the second's Wednesday.
    let (_, next) = run!(application::cycling::next_ride(
        &opened.cycling,
        date(2026, 10, 19)
    ));
    assert_eq!(next.date, date(2026, 10, 21));
    assert_eq!(next.microcycle, 1);

    // And inside a mesocycle, the ride found is that mesocycle's own.
    let (_, next) = run!(application::cycling::next_ride(
        &opened.cycling,
        date(2026, 10, 12)
    ));
    assert_eq!(next.date, date(2026, 10, 14));
    assert_eq!(next.microcycle, 4, "the fourth week of the first mesocycle");
}

/// Nothing authored is the ordinary first-run state, and it is reported rather
/// than guessed at.
#[test]
fn an_unauthored_store_holds_no_cycling_mesocycle() {
    let (opened, _directory) = opened!();
    assert!(run!(opened.cycling.on(date(2026, 9, 23))).is_none());
    assert!(run!(opened.plans.windows()).is_empty());
}
