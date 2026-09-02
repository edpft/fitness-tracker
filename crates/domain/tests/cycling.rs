//! The cycling domain, through its public surface.
//!
//! **The seed test is the one that earns its keep.** Every session in
//! *Peak Your Power Zones* was transcribed from a screenshot, and the app states
//! two things about each class that the transcription must reproduce: how long
//! the ride runs, and how many movements it contains. Both are checked here for
//! all twenty-five, against numbers read off the app rather than computed from
//! the intervals — so a dropped, duplicated or mistyped interval fails rather
//! than quietly becoming the programme.
//!
//! That is not hypothetical. The transcription was stitched across two
//! overlapping captures for the Max Ride, and one supplied `classId` turned out
//! to belong to a different class; both were caught by exactly these two
//! numbers disagreeing.

use domain::{
    cycling::{
        CycleDay, CyclingSession, Ftp, FtpProvenance, PowerZone, Ride, Selection, Watts,
        peak_your_power_zones,
    },
    gym::PositiveDuration,
};
use jiff::civil::{Weekday, date};

/// What the app stated for each class: warm-up minutes, ride seconds, movements.
///
/// Read off the operator's screenshots, never derived from the intervals — the
/// point is that two independent readings agree.
const STATED: [(u8, u8, u64, u64, usize); 24] = [
    (1, 1, 11, 1980, 7),
    (1, 3, 10, 2040, 7),
    (1, 6, 13, 2760, 11),
    (2, 1, 11, 1980, 11),
    (2, 3, 12, 1920, 5),
    (2, 6, 12, 2819, 11),
    (3, 1, 12, 1920, 25),
    (3, 3, 12, 1918, 11),
    (3, 6, 12, 2820, 9),
    (4, 1, 12, 1920, 9),
    (4, 3, 10, 2040, 7),
    (4, 6, 13, 2756, 9),
    (5, 1, 12, 1920, 11),
    (5, 3, 11, 1980, 11),
    (5, 6, 11, 2880, 9),
    (6, 1, 13, 1860, 15),
    (6, 3, 13, 1860, 11),
    (6, 6, 13, 2760, 14),
    (7, 1, 12, 1920, 33),
    (7, 3, 13, 1860, 31),
    (7, 6, 13, 4560, 19),
    (8, 1, 13, 1861, 5),
    (8, 3, 13, 1860, 9),
    (8, 6, 10, 1200, 0),
];

/// The session at a week and day, or `None` if this build does not hold one.
///
/// Fallible and unwrapped at each call site: the test exemptions for `expect`
/// reach a `#[test]` function and not a helper beside it (`CLAUDE.md`).
fn session(week: u8, day: u8) -> Option<CyclingSession> {
    let programme = peak_your_power_zones().ok()?;
    let day = CycleDay::new(day).ok()?;
    programme.week(usize::from(week))?.session(day).cloned()
}

#[test]
fn the_shipped_programme_builds() {
    let programme = peak_your_power_zones().expect("the shipped programme builds");
    assert_eq!(programme.name().as_str(), "Peak Your Power Zones");
    assert_eq!(programme.duration_weeks(), 8, "eight weeks");

    let sessions: usize = (1..=8)
        .filter_map(|week| programme.week(week))
        .map(|week| week.sessions().len())
        .sum();
    assert_eq!(sessions, 24, "three sessions a week for eight weeks");
}

#[test]
fn every_session_matches_the_duration_the_app_stated() {
    for (week, day, _, ride_seconds, _) in STATED {
        let session = session(week, day).expect("the session is in the programme");
        assert_eq!(
            session.ride().duration().as_seconds(),
            ride_seconds,
            "week {week} day {day}: the ride must total what the app stated",
        );
    }
}

#[test]
fn every_session_matches_the_movement_count_the_app_stated() {
    for (week, day, _, _, movements) in STATED {
        let session = session(week, day).expect("the session is in the programme");
        let counted = match session.ride() {
            Ride::Intervals(intervals) => intervals.count(),
            // The FTP test states no movements at all, which is the point of it.
            Ride::Effort(_) => 0,
        };
        assert_eq!(
            counted, movements,
            "week {week} day {day}: the interval count must match the app's movements",
        );
    }
}

#[test]
fn every_session_matches_the_warm_up_the_app_stated() {
    for (week, day, warm_up_minutes, _, _) in STATED {
        let session = session(week, day).expect("the session is in the programme");
        assert_eq!(
            session.warm_up().as_seconds(),
            warm_up_minutes * 60,
            "week {week} day {day}: warm-up",
        );
    }
}

#[test]
fn the_ftp_test_prescribes_a_duration_and_no_zone() {
    let session = session(8, 6).expect("week 8 day 6 is in the programme");
    let Ride::Effort(duration) = session.ride() else {
        panic!("week 8 day 6 is an effort, not an interval sequence");
    };
    assert_eq!(duration.as_seconds(), 1200, "twenty minutes");
    assert!(
        session.ride().time_in_zone().is_empty(),
        "an effort belongs to no zone until it has been ridden",
    );
    assert_eq!(
        session.ride().peak_zone(),
        None,
        "an effort names no zone to peak at",
    );
    assert_eq!(
        session.cool_down(),
        None,
        "the test class ships with no cool-down at all",
    );
}

#[test]
fn only_the_max_ride_reaches_zone_seven() {
    let reaching: Vec<_> = STATED
        .into_iter()
        .filter(|(week, day, ..)| {
            session(*week, *day)
                .expect("the session is in the programme")
                .ride()
                .peak_zone()
                == Some(PowerZone::Seven)
        })
        .map(|(week, day, ..)| (week, day))
        .collect();
    assert_eq!(
        reaching,
        vec![(7, 1)],
        "Z7 appears in the week 7 Max Ride and nowhere else",
    );
}

#[test]
fn week_four_reproduces_week_one_s_aerobic_dose() {
    let zone_three = |week: u8| -> u64 {
        [1_u8, 3, 6]
            .into_iter()
            .filter_map(|day| {
                session(week, day)
                    .expect("the session is in the programme")
                    .ride()
                    .time_in_zone()
                    .into_iter()
                    .find(|(zone, _)| *zone == PowerZone::Three)
                    .map(|(_, seconds)| seconds)
            })
            .sum()
    };
    let (one, four) = (zone_three(1), zone_three(4));
    assert_eq!(one, 4919, "week 1 runs 81:59 of Z3");
    assert_eq!(four, 4918, "week 4 runs 81:58 of Z3");
    assert!(
        one.abs_diff(four) <= 1,
        "the deload re-runs the base week's aerobic dose, to the second",
    );
}

#[test]
fn the_recovery_weeks_are_the_only_ones_with_nothing_above_zone_three() {
    let hard = |week: u8| -> u64 {
        [1_u8, 3, 6]
            .into_iter()
            .flat_map(|day| {
                session(week, day)
                    .expect("the session is in the programme")
                    .ride()
                    .time_in_zone()
            })
            .filter(|(zone, _)| *zone >= PowerZone::Four)
            .map(|(_, seconds)| seconds)
            .sum()
    };
    let easy: Vec<u8> = (1..=8).filter(|week| hard(*week) == 0).collect();
    assert_eq!(
        easy,
        vec![1, 4, 8],
        "the base week and the two recovery weeks, and no others",
    );
}

#[test]
fn a_zone_is_a_share_of_ftp() {
    let ftp = Ftp::new(
        Watts::from_u32(172),
        date(2026, 7, 22),
        FtpProvenance::Estimated,
    )
    .expect("172 watts is a threshold");

    let band = PowerZone::Four.band().watts_at(ftp);
    assert_eq!(band.lower().map(Watts::as_u32), Some(156), "91% of 172");
    assert_eq!(band.upper().map(Watts::as_u32), Some(180), "105% of 172");

    let recovery = PowerZone::One.band().watts_at(ftp);
    assert_eq!(recovery.lower(), None, "zone one has no floor");
    assert_eq!(recovery.upper().map(Watts::as_u32), Some(94));

    let sprint = PowerZone::Seven.band().watts_at(ftp);
    assert_eq!(sprint.upper(), None, "zone seven has no ceiling");
}

#[test]
fn the_operator_rides_wednesday_and_sunday() {
    let selection = Selection::new(vec![
        (
            Weekday::Wednesday,
            CycleDay::new(1).expect("day one is a day"),
        ),
        (Weekday::Sunday, CycleDay::new(6).expect("day six is a day")),
    ])
    .expect("two days is a selection");

    // 2026-09-16 is a Wednesday, 2026-09-20 the Sunday after it.
    assert_eq!(
        selection.cycle_day(date(2026, 9, 16)).map(CycleDay::as_u8),
        Some(1),
    );
    assert_eq!(
        selection.cycle_day(date(2026, 9, 20)).map(CycleDay::as_u8),
        Some(6),
        "Sunday morning takes day 6, the long ride",
    );
    assert_eq!(
        selection.cycle_day(date(2026, 9, 18)),
        None,
        "Friday is the gym's",
    );
}

#[test]
fn the_operator_s_own_cool_down_is_added_to_peloton_s() {
    let session = session(1, 1).expect("week 1 day 1 is in the programme");
    assert_eq!(
        session.cool_down().map(PositiveDuration::as_seconds),
        Some(60)
    );

    let extended =
        session.with_extra_cool_down(PositiveDuration::from_seconds(300).expect("five minutes"));
    assert_eq!(
        extended.cool_down().map(PositiveDuration::as_seconds),
        Some(360),
        "his five minutes on top of Peloton's one",
    );
    assert_eq!(
        extended.total().as_seconds(),
        session.total().as_seconds() + 300,
        "and the session costs five minutes more",
    );
}

#[test]
fn the_test_session_gains_a_cool_down_it_did_not_have() {
    let extended = session(8, 6)
        .expect("week 8 day 6 is in the programme")
        .with_extra_cool_down(PositiveDuration::from_seconds(300).expect("five minutes"));
    assert_eq!(
        extended.cool_down().map(PositiveDuration::as_seconds),
        Some(300),
        "absent plus five minutes is five minutes, not six",
    );
}
