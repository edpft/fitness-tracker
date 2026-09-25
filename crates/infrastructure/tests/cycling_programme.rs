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
        CyclingMesocycle, CyclingMicrocycle, CyclingProvenance, CyclingSession, Interval,
        PlannedRide, PowerZone, Ride, RideVenue, SessionPosition,
    },
    measure::PositiveDuration,
    plan::{Plan, PlanName, Programme},
    provider::{ExternalProgramme, ProgrammeName, Provider, PublishedAt},
    schedule::{Diary, PartOfDay, Relative, SessionRole, TrainingWeek},
    sequence::NonEmpty,
};
use infrastructure::{
    SqliteCyclingMesocycleStore, SqliteGenerationParameterStore, SqlitePlanStore, connect,
};
use jiff::civil::{Date, Weekday, date};
use support::{corpus, programme as gym};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// The three stores one plan is written and read through.
struct Opened {
    plans: SqlitePlanStore,
    cycling: SqliteCyclingMesocycleStore,
    parameters: SqliteGenerationParameterStore,
}

async fn store() -> Fallible<(Opened, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    Ok((
        Opened {
            plans: SqlitePlanStore::new(pool.clone(), corpus::zone()?),
            cycling: SqliteCyclingMesocycleStore::new(pool.clone()),
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
    let (_, authored) =
        application::prescribe::Authoring::new(opened.plans.clone(), opened.parameters.clone())
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
fn intervals(
    reference: &str,
    called: &str,
    published: PublishedAt,
    role: SessionRole,
) -> Fallible<PlannedRide> {
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
    Ok(PlannedRide::provided(
        session,
        NonEmpty::of(RideVenue::new(reference, called)?, Vec::new()),
        published,
        role,
    ))
}

/// The FTP warm-up and the test itself: one session, two places, no cool-down.
fn ftp_test(published: PublishedAt) -> Fallible<PlannedRide> {
    let session = CyclingSession::new(
        PositiveDuration::from_seconds(600)?,
        Ride::Effort(PositiveDuration::from_seconds(1200)?),
        None,
    );
    Ok(PlannedRide::provided(
        session,
        NonEmpty::of(
            RideVenue::new("1eabf70b20744f48b99259f93889ced5", "10 min FTP Warmup Ride")?,
            vec![RideVenue::new(
                "4d302bef49574118a071269bed38bd30",
                "20 min FTP Test Ride",
            )?],
        ),
        published,
        // The test is the week's higher-intensity, shorter ride, whatever
        // Peloton numbers it.
        SessionRole::new(Relative::Higher, Relative::Lower),
    ))
}

fn microcycle(published: u32, test: bool) -> Fallible<CyclingMicrocycle> {
    // **A test week rides the same two roles as any other.** The test is the
    // shorter, harder ride and takes the Wednesday like every week's harder
    // session; Peloton just files it second, where an ordinary week's shorter
    // ride is first. Issue #63: a role places a ride, a published order does
    // not.
    let (first_role, second_role) = if test {
        (
            SessionRole::new(Relative::Lower, Relative::Higher),
            SessionRole::new(Relative::Higher, Relative::Lower),
        )
    } else {
        (
            SessionRole::new(Relative::Higher, Relative::Lower),
            SessionRole::new(Relative::Lower, Relative::Higher),
        )
    };
    let second = if test {
        ftp_test(PublishedAt::new(published, 3)?)?
    } else {
        intervals(
            "414a518108ea4c5cada00ab9899a9d8d",
            "60 min Power Zone Ride",
            PublishedAt::new(published, 3)?,
            second_role,
        )?
    };
    let rides = [
        (
            SessionPosition::new(1)?,
            intervals(
                "9f8f3af689cc4f0db9afa013d4676ed6",
                "45 min Power Zone Endurance Ride",
                PublishedAt::new(published, 1)?,
                first_role,
            )?,
        ),
        // The operator's second ride of the week, taken from the published
        // third — numbered as his, with the published number kept beside it.
        (SessionPosition::new(2)?, second),
    ];
    Ok(CyclingMicrocycle::new(rides.into_iter().collect())?)
}

fn programme(start: Date, published: &[u32], test: bool) -> Fallible<CyclingMesocycle> {
    let weeks = published
        .iter()
        .map(|number| microcycle(*number, test))
        .collect::<Fallible<Vec<_>>>()?;
    Ok(CyclingMesocycle::new(
        CyclingProvenance::Provided(ExternalProgramme::new(
            Provider::try_from("Peloton".to_owned())?,
            ProgrammeName::try_from("Build Your Power Zones".to_owned())?,
        )),
        start,
        NonEmpty::new(weeks)?,
    )?)
}

/// The week cycling is given: Wednesday the harder, shorter ride, Sunday the
/// easier, longer one. The operator's own, and what the programme used to
/// carry a copy of.
fn riding_week() -> Fallible<TrainingWeek> {
    Ok(TrainingWeek::new(vec![
        (
            Weekday::Wednesday,
            SessionRole::new(Relative::Higher, Relative::Lower),
        ),
        (
            Weekday::Sunday,
            SessionRole::new(Relative::Lower, Relative::Higher),
        ),
    ])?)
}

/// A diary that says nothing: no patterns, no alterations, no absences.
///
/// **What "no illness that week" looks like.** The easing rule (#180) reads the
/// diary for a cycling slot lost to illness; an empty one has none, so these
/// suites get the ordinary role. The week is supplied separately because that
/// is how `next_ride` takes it.
fn quiet_diary() -> Diary {
    Diary::new(Vec::new(), Vec::new())
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
///
/// **And the test is ridden on the Wednesday.** Peloton files it second, which
/// by published order is the Sunday; it is the higher-intensity, shorter
/// session, and the Wednesday is that slot (issue #63).
#[test]
fn an_effort_round_trips_with_no_cool_down() {
    let (opened, _directory) = opened!();
    let Ok(authored) = programme(date(2026, 9, 14), &[5], true) else {
        panic!("the fixture mesocycle is valid")
    };
    let Ok(plan) = plan("autumn", vec![authored]) else {
        panic!("the fixture plan is valid")
    };
    let Ok(week) = riding_week() else {
        panic!("the fixture week is valid")
    };

    run!(author(&opened, &plan));

    let Some((_, _, read_back)) = run!(opened.cycling.on(date(2026, 9, 16))) else {
        panic!("the test week covers its Wednesday")
    };
    let Some((_, position, ride)) = read_back.on(date(2026, 9, 16), &week) else {
        panic!("Wednesday rides the test")
    };
    assert_eq!(
        position.as_u8(),
        2,
        "the test is the week's second session and still takes the Wednesday"
    );
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
        date(2026, 10, 19),
        &riding_week().expect("the fixture week is valid"),
        &quiet_diary()
    ));
    let next = next.ride().expect("a ride of the programme is due");
    assert_eq!(next.date, date(2026, 10, 21));
    assert_eq!(next.microcycle, 1);

    // And inside a mesocycle, the ride found is that mesocycle's own.
    let (_, next) = run!(application::cycling::next_ride(
        &opened.cycling,
        date(2026, 10, 12),
        &riding_week().expect("the fixture week is valid"),
        &quiet_diary()
    ));
    let next = next.ride().expect("a ride of the programme is due");
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

/// A holding week: two 45-minute classes, roled by kind rather than by length.
///
/// **The same duration on purpose.** The operator named a 45-minute Power Zone
/// ride and a 45-minute Power Zone Endurance ride on 2026-09-20, and the role
/// comparison admits equality so that the week is expressible at all. A fixture
/// of two different lengths would pass whether the roles were stated or derived.
fn holding(start: Date) -> Fallible<CyclingMesocycle> {
    let ride = |reference: &str, called: &str, role| -> Fallible<PlannedRide> {
        let session = CyclingSession::new(
            PositiveDuration::from_seconds(300)?,
            Ride::Intervals(NonEmpty::new(vec![Interval::new(
                PowerZone::Three,
                PositiveDuration::from_seconds(2_400)?,
            )])?),
            None,
        );
        Ok(PlannedRide::assembled(
            session,
            NonEmpty::of(RideVenue::new(reference, called)?, Vec::new()),
            role,
        ))
    };

    let week = CyclingMicrocycle::new(
        [
            (
                SessionPosition::new(1)?,
                ride(
                    "newest-power-zone",
                    "45 min Power Zone Ride",
                    SessionRole::new(Relative::Higher, Relative::Lower),
                )?,
            ),
            (
                SessionPosition::new(2)?,
                ride(
                    "newest-endurance",
                    "45 min Power Zone Endurance Ride",
                    SessionRole::new(Relative::Lower, Relative::Higher),
                )?,
            ),
        ]
        .into_iter()
        .collect(),
    )?;

    Ok(CyclingMesocycle::new(
        CyclingProvenance::Assembled,
        start,
        NonEmpty::new(vec![week])?,
    )?)
}

/// **A holding week survives the store, and still names no programme.**
///
/// Everything else authored here is microcycles of *Build Your Power Zones*;
/// this is two classes picked out of the catalogue because the gym is repeating
/// a week (#180). The provenance is the whole difference, and it has to come
/// back as it went in — a holding week that read back as `Provided` would claim
/// a programme nobody published and offer a way back to a microcycle that does
/// not exist.
#[test]
fn a_holding_week_round_trips_naming_no_programme() {
    let (opened, _directory) = opened!();
    let Ok(authored) = holding(date(2026, 9, 21)) else {
        panic!("the fixture holding week is valid")
    };
    let Ok(plan) = plan("autumn", vec![authored.clone()]) else {
        panic!("the fixture plan is valid")
    };

    run!(author(&opened, &plan));

    let read = run!(opened.cycling.on(date(2026, 9, 23)));
    let Some((_, _, held)) = read else {
        panic!("the holding week covers the Wednesday")
    };

    assert_eq!(held, authored, "exactly as authored");
    assert_eq!(held.provenance(), &CyclingProvenance::Assembled);
    assert_eq!(held.programme(), None, "nobody published this week");
    assert_eq!(held.provided_from(), None);
    let Some(week) = held.microcycle(1) else {
        panic!("a holding mesocycle has one microcycle")
    };
    assert_eq!(
        week.published_ordinal(),
        None,
        "it is not the nth week of anything"
    );
    for ride in week.rides().values() {
        assert_eq!(ride.published(), None, "and no ride names a published one");
    }
}

/// **The holding week's Wednesday rides the harder class, and the Sunday the
/// easier one** — the two rides being the same length.
///
/// This is #180's acceptance line. What places each ride is its role meeting a
/// slot of the same role (#63); with equal durations nothing else could.
#[test]
fn a_holding_week_puts_the_power_zone_ride_on_the_wednesday() {
    let (opened, _directory) = opened!();
    let Ok(authored) = holding(date(2026, 9, 21)) else {
        panic!("the fixture holding week is valid")
    };
    let Ok(plan) = plan("autumn", vec![authored]) else {
        panic!("the fixture plan is valid")
    };
    run!(author(&opened, &plan));

    let week = riding_week().expect("the fixture week is valid");

    let (_, wednesday) = run!(application::cycling::next_ride(
        &opened.cycling,
        date(2026, 9, 21),
        &week,
        &quiet_diary()
    ));
    let wednesday = wednesday.ride().expect("a ride of the programme is due");
    assert_eq!(wednesday.date, date(2026, 9, 23));
    assert_eq!(
        wednesday.ride.at().first().called(),
        "45 min Power Zone Ride"
    );

    let (_, sunday) = run!(application::cycling::next_ride(
        &opened.cycling,
        date(2026, 9, 24),
        &week,
        &quiet_diary()
    ));
    let sunday = sunday.ride().expect("a ride of the programme is due");
    assert_eq!(sunday.date, date(2026, 9, 27));
    assert_eq!(
        sunday.ride.at().first().called(),
        "45 min Power Zone Endurance Ride"
    );
}

/// **With the Sunday lost to illness, the surviving Wednesday rides the easier
/// class.**
///
/// The operator, 2026-09-20: *"if one session was lost to illness, the
/// remaining session would be easier."* The slot still asks for the
/// higher-intensity role; the week answers with the lower-intensity ride
/// because a session that week was lost to illness. #180's second acceptance
/// line.
#[test]
fn an_illness_that_week_eases_the_surviving_wednesday() {
    let (opened, _directory) = opened!();
    let Ok(authored) = holding(date(2026, 9, 21)) else {
        panic!("the fixture holding week is valid")
    };
    let Ok(plan) = plan("autumn", vec![authored]) else {
        panic!("the fixture plan is valid")
    };
    run!(author(&opened, &plan));

    let Some(one) = std::num::NonZeroU8::new(1) else {
        panic!("one is not zero")
    };
    let Ok(zone) = domain::normalised::OperatorZone::try_from("Europe/London") else {
        panic!("Europe/London is a zone")
    };
    let slots: std::collections::BTreeMap<_, _> = [
        (
            domain::schedule::TrainingSlot::new(Weekday::Wednesday, PartOfDay::Evening),
            domain::schedule::Allocation::new(
                domain::schedule::Discipline::Cycling,
                SessionRole::new(Relative::Higher, Relative::Lower),
            ),
        ),
        (
            domain::schedule::TrainingSlot::new(Weekday::Sunday, PartOfDay::Morning),
            domain::schedule::Allocation::new(
                domain::schedule::Discipline::Cycling,
                SessionRole::new(Relative::Lower, Relative::Higher),
            ),
        ),
    ]
    .into_iter()
    .collect();

    let diary = Diary::new(
        vec![domain::schedule::TrainingPattern::new(
            date(2026, 1, 1),
            zone,
            slots,
        )],
        vec![domain::schedule::Alteration::new(
            date(2026, 9, 27),
            one,
            domain::schedule::Absence::Illness,
        )],
    );

    let (_, wednesday) = run!(application::cycling::next_ride(
        &opened.cycling,
        date(2026, 9, 21),
        &riding_week().expect("the fixture week is valid"),
        &diary
    ));
    let wednesday = wednesday.ride().expect("a ride of the programme is due");
    assert_eq!(wednesday.date, date(2026, 9, 23));
    assert_eq!(
        wednesday.ride.at().first().called(),
        "45 min Power Zone Endurance Ride",
        "the Sunday was lost to illness, so the Wednesday rides the easier class"
    );
}
