//! The authored cycling programme, through its real store (§ 12).
//!
//! **The point of storing the zone plan is that it comes back exactly.** A ride
//! read from Peloton in September is prescribed in December from these rows and
//! from nothing else — no re-fetch, no network, and § 13's reproducibility is
//! whatever this round trip preserves. So the assertion is equality of the whole
//! programme rather than of a summary of it.

mod support;

use application::CyclingProgrammeStore as _;
use domain::{
    cycling::{
        CyclingMicrocycle, CyclingProgramme, CyclingSession, CyclingWeekdays, Interval,
        PlannedRide, PowerZone, PublishedMicrocycle, Ride, RideVenue, SessionPosition,
    },
    gym::{PositiveDuration, sequence::NonEmpty},
    prescription::ProgrammeName,
};
use infrastructure::{SqliteCyclingProgrammeStore, connect};
use jiff::civil::{Date, Weekday, date};
use support::corpus;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

async fn store() -> Fallible<(SqliteCyclingProgrammeStore, tempfile::TempDir)> {
    let directory = tempfile::tempdir()?;
    let pool = connect(&directory.path().join("test.db")).await?;
    Ok((SqliteCyclingProgrammeStore::new(pool), directory))
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
fn intervals(reference: &str, called: &str) -> Fallible<PlannedRide> {
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
    ))
}

fn microcycle(published: u32, test: bool) -> Fallible<CyclingMicrocycle> {
    let second = if test {
        ftp_test()?
    } else {
        intervals("414a518108ea4c5cada00ab9899a9d8d", "60 min Power Zone Ride")?
    };
    let rides = [
        (
            SessionPosition::new(1)?,
            intervals(
                "9f8f3af689cc4f0db9afa013d4676ed6",
                "45 min Power Zone Endurance Ride",
            )?,
        ),
        (SessionPosition::new(3)?, second),
    ];
    Ok(CyclingMicrocycle::new(
        rides.into_iter().collect(),
        PublishedMicrocycle::new(ProgrammeName::try_from("Power Zone Build")?, published),
    )?)
}

fn programme(name: &str, start: Date, published: &[u32], test: bool) -> Fallible<CyclingProgramme> {
    let weeks = published
        .iter()
        .map(|number| microcycle(*number, test))
        .collect::<Fallible<Vec<_>>>()?;
    let weekdays = CyclingWeekdays::new(vec![
        (Weekday::Wednesday, SessionPosition::new(1)?),
        (Weekday::Sunday, SessionPosition::new(3)?),
    ])?;
    Ok(CyclingProgramme::new(
        ProgrammeName::try_from(name)?,
        jiff::Timestamp::now(),
        start,
        NonEmpty::new(weeks)?,
        weekdays,
    )?)
}

/// Every interval, every venue and the order of both, exactly as authored.
#[test]
fn an_authored_programme_round_trips_exactly() {
    let (store, _directory) = opened!();
    let Ok(authored) = programme("autumn-cycling-1", date(2026, 9, 21), &[1, 2, 4, 5], false)
    else {
        panic!("the fixture programme is valid")
    };

    run!(store.author(&authored));

    let Some((_, read_back)) = run!(store.on(date(2026, 9, 23))) else {
        panic!("the programme covers the date it was authored for")
    };
    assert_eq!(read_back, authored, "the whole programme, not a summary");
}

/// A ride with no zones and no cool-down is the FTP test, and absent must not
/// come back as zero: they are different claims and only one is true.
#[test]
fn an_effort_round_trips_with_no_cool_down() {
    let (store, _directory) = opened!();
    let Ok(authored) = programme("autumn-cycling-test", date(2026, 9, 14), &[5], true) else {
        panic!("the fixture programme is valid")
    };

    run!(store.author(&authored));

    let Some((_, read_back)) = run!(store.on(date(2026, 9, 20))) else {
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

/// Versions of one programme sit on top of one another; the latest answers.
#[test]
fn re_authoring_a_name_supersedes_rather_than_competing() {
    let (store, _directory) = opened!();
    let Ok(first) = programme("autumn-cycling-1", date(2026, 9, 21), &[1, 2, 4, 5], false) else {
        panic!("the fixture programme is valid")
    };
    let Ok(corrected) = programme("autumn-cycling-1", date(2026, 9, 21), &[1, 2, 3, 4], false)
    else {
        panic!("the corrected programme is valid")
    };

    let (_, created) = run!(application::cycling::author(&store, &first));
    let (_, modified) = run!(application::cycling::author(&store, &corrected));

    assert_eq!(created, application::Authored::Created);
    assert_eq!(
        modified,
        application::Authored::Modified,
        "the same name is a re-authoring, not a rival",
    );

    let Some((_, read_back)) = run!(store.on(date(2026, 9, 23))) else {
        panic!("the programme covers the date")
    };
    assert_eq!(read_back, corrected, "the latest authoring answers");
}

/// Two cycling programmes covering one day would make which of them answers
/// depend on the order rows came back in.
#[test]
fn a_second_programme_covering_the_same_days_is_refused() {
    let (store, _directory) = opened!();
    let Ok(first) = programme("autumn-cycling-1", date(2026, 9, 21), &[1, 2, 4, 5], false) else {
        panic!("the fixture programme is valid")
    };
    let Ok(overlapping) = programme("autumn-cycling-2", date(2026, 10, 12), &[1, 2, 4, 5], false)
    else {
        panic!("the overlapping programme is valid")
    };

    run!(application::cycling::author(&store, &first));

    let Ok(refused) = corpus::block_on(application::cycling::author(&store, &overlapping)) else {
        panic!("a runtime is available")
    };
    assert!(
        matches!(
            refused,
            Err(application::PrescriptionError::OverlappingProgramme { .. })
        ),
        "one day, two programmes",
    );
}

/// The autumn authors four cycling programmes back to back, so a question asked
/// after the last ride of one has its answer in the next.
#[test]
fn the_next_ride_crosses_a_mesocycle_boundary() {
    let (store, _directory) = opened!();
    let Ok(first) = programme("autumn-cycling-1", date(2026, 9, 21), &[1, 2, 4, 5], false) else {
        panic!("the fixture programme is valid")
    };
    let Ok(second) = programme("autumn-cycling-2", date(2026, 10, 19), &[1, 2, 4, 5], false) else {
        panic!("the second programme is valid")
    };

    run!(application::cycling::author(&store, &first));
    run!(application::cycling::author(&store, &second));

    // Monday 2026-10-19 opens the second mesocycle; asked from the Monday
    // after the first one's last Sunday, the answer is the second's Wednesday.
    let (programme, next) = run!(application::cycling::next_ride(&store, date(2026, 10, 19)));
    assert_eq!(programme.name().as_str(), "autumn-cycling-2");
    assert_eq!(next.date, date(2026, 10, 21));
    assert_eq!(next.microcycle, 1);

    // And inside a mesocycle, the ride found is that programme's own.
    let (programme, next) = run!(application::cycling::next_ride(&store, date(2026, 10, 12)));
    assert_eq!(programme.name().as_str(), "autumn-cycling-1");
    assert_eq!(next.date, date(2026, 10, 14));
    assert_eq!(next.microcycle, 4, "the fourth week of the first mesocycle");
}

/// Nothing authored is the ordinary first-run state, and it is reported rather
/// than guessed at.
#[test]
fn an_unauthored_store_holds_no_cycling_programme() {
    let (store, _directory) = opened!();
    assert!(run!(store.on(date(2026, 9, 23))).is_none());
    assert!(run!(store.windows()).is_empty());
}
