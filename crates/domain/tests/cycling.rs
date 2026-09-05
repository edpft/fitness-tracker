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
        CycleDay, CyclingSession, Ftp, FtpProvenance, Interval, PowerZone, Ride, Selection, Watts,
        ZoneProfile, bottom_level, diverges, mesocycles, peak_your_power_zones, span, zones_lost,
    },
    gym::{PositiveDuration, sequence::NonEmpty},
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

/// A ride of one stretch at one zone.
///
/// Fallible and unwrapped at each call site: the test exemptions for `expect`
/// reach a `#[test]` function and not a helper beside it (`CLAUDE.md`).
fn held(zone: PowerZone, seconds: u64) -> Result<Ride, Box<dyn std::error::Error>> {
    let duration = PositiveDuration::from_seconds(seconds)?;
    Ok(Ride::Intervals(NonEmpty::new(vec![Interval::new(
        zone, duration,
    )])?))
}

#[test]
fn an_hour_at_a_zone_scores_the_intensity_that_zone_names() {
    let ride = held(PowerZone::Four, 3600).expect("an hour of zone four is a ride");

    // Zone four spans 91-105% of FTP, so it is scored at 98%. An hour at an
    // intensity factor of 0.98 is 0.98² × 100 = 96.04 — the definition of TSS
    // rather than anything fitted here.
    let scored = ZoneProfile::of([&ride]).tss();

    assert!(
        (scored - 96.04).abs() < 0.005,
        "an hour at zone four should score 96.04, scored {scored}"
    );
}

#[test]
fn the_open_ended_zones_are_scored_at_a_stated_intensity() {
    // Zone one has no floor and zone seven has no ceiling, so neither has a
    // midpoint and each is given one. **Pinned because they are invented**:
    // changing either should be a decision, not a diff nobody noticed.
    let one = held(PowerZone::One, 3600).expect("an hour of zone one is a ride");
    let seven = held(PowerZone::Seven, 3600).expect("an hour of zone seven is a ride");

    let (scored_one, scored_seven) = (
        ZoneProfile::of([&one]).tss(),
        ZoneProfile::of([&seven]).tss(),
    );

    assert!(
        (scored_one - 20.25).abs() < 0.005,
        "zone one is scored at 45% of FTP, so an hour is 20.25 — scored {scored_one}"
    );
    assert!(
        (scored_seven - 289.0).abs() < 0.005,
        "zone seven is scored at 170% of FTP, so an hour is 289 — scored {scored_seven}"
    );
}

#[test]
fn equal_riding_at_zone_two_and_zone_three_scores_differently() {
    // **This is the whole reason TSS was added.** Boost Your Base is entirely
    // Z1/Z2/Z3, so hard share reports zero for every microcycle of it and finds
    // no structure at all — while the programme builds by shifting Z2 toward Z3.
    let steady = held(PowerZone::Two, 1800).expect("half an hour of zone two is a ride");
    let tempo = held(PowerZone::Three, 1800).expect("half an hour of zone three is a ride");

    let (steady, tempo) = (ZoneProfile::of([&steady]), ZoneProfile::of([&tempo]));

    assert!(
        steady.hard_share().abs() < f64::EPSILON && tempo.hard_share().abs() < f64::EPSILON,
        "neither reaches zone four, which is exactly what makes hard share blind here"
    );
    assert!(
        tempo.tss() > steady.tss(),
        "the same half hour at tempo should score above endurance: {} against {}",
        tempo.tss(),
        steady.tss()
    );
}

#[test]
fn the_ftp_test_scores_nothing() {
    // It measures the number every zone is a share of, so it has no intensity
    // of its own to score — the same reason it contributes no zone share.
    let duration = PositiveDuration::from_seconds(1200).expect("twenty minutes is a duration");
    let test = Ride::Effort(duration);

    let scored = ZoneProfile::of([&test]).tss();

    assert!(
        scored.abs() < f64::EPSILON,
        "an effort names no zone, so it scores nothing — scored {scored}"
    );
}

/// TSS per microcycle, read from the Peloton API on 2026-09-05 by
/// `infrastructure/examples/transcribe.rs`. All three sessions of each.
const BASE: [f64; 8] = [50.0, 86.0, 111.0, 49.0, 116.0, 131.0, 148.0, 79.0];
const PEAK: [f64; 8] = [114.0, 124.0, 126.0, 113.0, 129.0, 132.0, 160.0, 61.0];
const BUILD: [f64; 5] = [108.0, 123.0, 129.0, 141.0, 63.0];

#[test]
fn the_bottom_level_is_the_operators_reading_of_each_shape() {
    // His own words for these, 2026-09-05: Peak µ1-4 is 1-2-2-1, Base µ1-4 is
    // 1-2-3-1, Peak µ5-8 is 2-3-4-1. Only where the 1s fall is derived here.
    assert_eq!(
        bottom_level(&PEAK[0..4]),
        [true, false, false, true],
        "1-2-2-1 opens and closes at the bottom"
    );
    assert_eq!(
        bottom_level(&BASE[0..4]),
        [true, false, false, true],
        "1-2-3-1 does too"
    );
    assert_eq!(
        bottom_level(&PEAK[4..8]),
        [false, false, false, true],
        "2-3-4-1 opens above the bottom"
    );
}

#[test]
fn reordering_within_a_level_does_not_change_the_shape() {
    // The operator, 2026-09-05: "if the numbers were the other way around and
    // they went 113, 126, 124, 114, they still would be" a 1-2-2-1. **This is
    // the test that says a level is not a rank**: swapping 113 with 114 and 124
    // with 126 exchanges the strict minimum and must change nothing.
    let stated = [114.0, 124.0, 126.0, 113.0];
    let reordered = [113.0, 126.0, 124.0, 114.0];

    assert_eq!(bottom_level(&stated), bottom_level(&reordered));
    assert_eq!(
        mesocycles(&stated, 4).len(),
        mesocycles(&reordered, 4).len()
    );
}

#[test]
fn every_programme_yields_the_mesocycles_it_is_said_to_have() {
    assert_eq!(
        mesocycles(&PEAK, 4),
        vec![0..4, 4..8],
        "Peak is two of four"
    );
    assert_eq!(mesocycles(&BASE, 4), vec![0..4, 4..8], "so is Base");

    // **Build is five microcycles answering four** (decision 0032), and only
    // µ2-5 qualifies: µ1-4 peaks last and so has no deload to end on.
    assert_eq!(mesocycles(&BUILD, 4), vec![1..5]);
}

#[test]
fn base_is_the_programme_a_threshold_count_cannot_see() {
    // Boost Your Base contains no zone four at all, so its hard shares are eight
    // zeros — the failure issue #71 opened on. Scored by TSS the same eight
    // microcycles carry two mesocycles.
    let hard_shares = [0.0; 8];

    assert!(
        mesocycles(&hard_shares, 4).is_empty(),
        "a flat row of zeros has no working microcycle, so no mesocycle"
    );
    assert_eq!(mesocycles(&BASE, 4).len(), 2);
}

#[test]
fn a_run_of_any_requested_length_can_be_asked_for() {
    // Issue #71 asks a programme for *n* microcycles, not always four.
    assert_eq!(
        mesocycles(&BUILD, 5),
        vec![0..5],
        "Build's own shape is five"
    );
    assert_eq!(
        mesocycles(&BUILD, 2),
        vec![3..5],
        "and its last two are one too"
    );
    assert!(
        mesocycles(&BUILD, 0).is_empty(),
        "a run of nothing is not a run"
    );
    assert!(
        mesocycles(&BUILD, 9).is_empty(),
        "nor is one longer than the programme"
    );
}

#[test]
fn the_two_axes_multiply_to_the_score() {
    // **An identity, not a calibration**: TSS is volume × intensity², so
    // carrying all three loses nothing and separates what the product hides.
    let ride = held(PowerZone::Four, 1800).expect("half an hour of zone four is a ride");
    let easy = held(PowerZone::Two, 2700).expect("three quarters of an hour of two is a ride");
    let profile = ZoneProfile::of([&ride, &easy]);

    #[expect(
        clippy::cast_precision_loss,
        reason = "seconds of riding; f64 is exact far past any plausible total"
    )]
    let from_axes = profile.total() as f64 * (profile.intensity() / 100.0).powi(2) / 36.0;

    assert!(
        (profile.tss() - from_axes).abs() < 1e-9,
        "{} should be the two axes multiplied, got {from_axes}",
        profile.tss()
    );
}

#[test]
fn volume_and_intensity_move_independently() {
    // The same intensity at two volumes, and the same volume at two
    // intensities. **This is what the product cannot tell apart** — Boost Your
    // Base raises volume at a flat intensity where Build raises intensity at a
    // flat volume, and both read as a rising TSS.
    let short = held(PowerZone::Three, 1800).expect("half an hour of three is a ride");
    let long = held(PowerZone::Three, 3600).expect("an hour of three is a ride");
    let hard = held(PowerZone::Five, 1800).expect("half an hour of five is a ride");

    let (short, long, hard) = (
        ZoneProfile::of([&short]),
        ZoneProfile::of([&long]),
        ZoneProfile::of([&hard]),
    );

    assert!(
        (short.intensity() - long.intensity()).abs() < 1e-9,
        "twice the riding at one zone is twice the volume at the same intensity"
    );
    assert_eq!(
        (short.total(), hard.total()),
        (1800, 1800),
        "and these two differ in intensity at one volume"
    );
    assert!(hard.intensity() > short.intensity());
    assert!(hard.tss() > short.tss() && long.tss() > short.tss());
}

#[test]
fn an_empty_profile_has_no_intensity_to_report() {
    // Not zero because it was easy — zero because there was nothing. A week of
    // rest has no shape, and the same reason `shares` is empty for it.
    let duration = PositiveDuration::from_seconds(1200).expect("twenty minutes is a duration");
    let test = Ride::Effort(duration);

    let profile = ZoneProfile::of([&test]);

    assert_eq!(profile.total(), 0);
    assert!(profile.intensity().abs() < f64::EPSILON);
}

#[test]
fn an_identical_composition_diverges_by_nothing_at_any_volume() {
    let half = held(PowerZone::Three, 1800).expect("half an hour of three is a ride");
    let full = held(PowerZone::Three, 3600).expect("an hour of three is a ride");

    let (half, full) = (ZoneProfile::of([&half]), ZoneProfile::of([&full]));

    assert!(
        diverges(&half, &full).abs() < f64::EPSILON,
        "twice the riding at one zone is the same composition"
    );
}

#[test]
fn a_dropped_zone_is_a_structural_fact_and_not_a_score() {
    // **Why `zones_lost` exists.** A reference that is mostly zone three with a
    // sliver of zone six: dropping every second of the zone six costs about
    // twice its share of the clock and nothing more, which is a rounding error
    // beside the scale of the other zones. Squaring and dividing by the zone's
    // own share does not rescue it — that charges *less* for a zone going
    // missing, not more. So the fact is carried separately.
    let bulk = held(PowerZone::Three, 3540).expect("fifty-nine minutes of three is a ride");
    let sliver = held(PowerZone::Six, 60).expect("a minute of six is a ride");
    let reference = ZoneProfile::of([&bulk, &sliver]);
    let without = ZoneProfile::of([&bulk]);

    assert!(
        diverges(&without, &reference) < 4.0,
        "losing a whole zone is cheap in percentage points, which is the point"
    );
    assert_eq!(
        zones_lost(&without, &reference),
        vec![PowerZone::Six],
        "and is not cheap at all when it is named rather than scored"
    );
    assert!(
        zones_lost(&reference, &without).is_empty(),
        "gaining a zone the reference lacks is not losing one"
    );
}

#[test]
fn span_is_a_ratio_and_so_survives_a_change_of_length() {
    // Build's four working microcycles against a three-microcycle selection
    // from them. **Both are describable; neither had to be resampled.**
    let written = [108.0, 123.0, 129.0, 141.0];
    let selected = [108.0, 123.0, 141.0];

    let (written, selected) = (
        span(&written).expect("the written programme climbs"),
        span(&selected).expect("so does the selection"),
    );

    assert!((written - 141.0 / 108.0).abs() < 1e-9);
    assert!(
        (written - selected).abs() < f64::EPSILON,
        "keeping both endpoints keeps the span: {written} against {selected}"
    );
}

#[test]
fn a_run_with_nothing_in_it_has_no_span() {
    assert_eq!(span(&[]), None);
    assert_eq!(
        span(&[0.0, 5.0]),
        None,
        "there is no ratio to take against zero"
    );
}
