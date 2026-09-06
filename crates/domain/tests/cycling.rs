//! The cycling domain, through its public surface.
//!
//! **The transcribed seed and its tests are gone**, and what replaced them is
//! two different things. What a class *contains* is read from the source and
//! checked where the source is — the transcription disagreed with the API by
//! about a minute in every zoned session, so a test pinning the transcription
//! was pinning the error. What an authored programme *is* — a mesocycle of
//! microcycles, a weekday map, and the rides those two resolve to — is checked
//! here, because it is the domain's own arithmetic and holds for any source.

use domain::{
    cycling::{
        CyclingMesocycle, CyclingMicrocycle, CyclingSession, CyclingWeekdays, Ftp, FtpProvenance,
        Interval, PlannedRide, PowerZone, PublishedMicrocycle, Ride, RideVenue, SessionPosition,
        Watts, ZoneProfile, bottom_level, diverges, mesocycles, partition, span, zones_lost,
    },
    gym::{PositiveDuration, sequence::NonEmpty},
    prescription::ProgrammeName,
};
use jiff::civil::{Weekday, date};

/// A ride of one zone held for a stated time, at one venue.
///
/// Fallible and unwrapped at each call site: the test exemptions for `expect`
/// reach a `#[test]` function and not a helper beside it (`CLAUDE.md`).
fn ride(
    zone: PowerZone,
    seconds: u64,
    called: &str,
    published_session: u32,
) -> Result<PlannedRide, Box<dyn std::error::Error>> {
    let duration = PositiveDuration::from_seconds(seconds)?;
    let session = CyclingSession::new(
        PositiveDuration::from_seconds(600)?,
        Ride::Intervals(NonEmpty::new(vec![Interval::new(zone, duration)])?),
        Some(PositiveDuration::from_seconds(60)?),
    );
    let venue = RideVenue::new("0bc8a790d8ca49cc8355cc7411842ca9", called)?;
    Ok(PlannedRide::new(
        session,
        NonEmpty::of(venue, Vec::new()),
        published_session,
    ))
}

/// A microcycle riding sessions 1 and 3, taken from a published microcycle.
fn microcycle(number: u32) -> Result<CyclingMicrocycle, Box<dyn std::error::Error>> {
    let rides = [
        (
            SessionPosition::new(1)?,
            ride(PowerZone::Two, 1800, "45 min Power Zone Endurance Ride", 1)?,
        ),
        (
            SessionPosition::new(2)?,
            ride(PowerZone::Four, 2400, "60 min Power Zone Ride", 3)?,
        ),
    ];
    Ok(CyclingMicrocycle::new(
        rides.into_iter().collect(),
        PublishedMicrocycle::new(ProgrammeName::try_from("Power Zone Build")?, number),
    )?)
}

/// Four microcycles beginning Monday 2026-09-21, ridden Wednesday and Sunday.
fn programme() -> Result<CyclingMesocycle, Box<dyn std::error::Error>> {
    let microcycles = NonEmpty::new(vec![
        microcycle(1)?,
        microcycle(2)?,
        microcycle(4)?,
        microcycle(5)?,
    ])?;
    let weekdays = CyclingWeekdays::new(vec![
        (Weekday::Wednesday, SessionPosition::new(1)?),
        (Weekday::Sunday, SessionPosition::new(2)?),
    ])?;
    Ok(CyclingMesocycle::new(
        ProgrammeName::try_from("cycling-1")?,
        jiff::Timestamp::now(),
        date(2026, 9, 21),
        microcycles,
        weekdays,
    )?)
}

/// The published numbering is kept, because it is the way back to what was not
/// chosen: an answer of µ1-2-4-5 drops the published third microcycle, and
/// nothing renumbers the rest to hide it.
#[test]
fn a_microcycle_says_which_published_one_it_is() {
    let programme = programme().expect("the fixture programme is valid");
    let numbers: Vec<u32> = programme
        .microcycles()
        .iter()
        .map(|microcycle| microcycle.from().microcycle())
        .collect();
    assert_eq!(numbers, vec![1, 2, 4, 5]);
    assert_eq!(
        programme
            .microcycle(3)
            .map(|microcycle| microcycle.from().to_string()),
        Some("Power Zone Build µ4".to_owned()),
        "the programme's third microcycle is the published fourth",
    );
}

/// The start date is the programme's, so a date resolves without being told
/// one: this is what `cycling next` stopped needing `--start` for.
#[test]
fn a_date_resolves_to_a_microcycle_and_a_ride() {
    let programme = programme().expect("the fixture programme is valid");

    // 2026-09-23 is the Wednesday of the first week, 2026-10-11 the Sunday of
    // the third.
    let (number, position, ride) = programme
        .on(date(2026, 9, 23))
        .expect("the first Wednesday rides");
    assert_eq!(number, 1);
    assert_eq!(position.as_u8(), 1);
    assert_eq!(
        ride.at().first().called(),
        "45 min Power Zone Endurance Ride"
    );

    let (number, position, ride) = programme
        .on(date(2026, 10, 11))
        .expect("the third Sunday rides");
    assert_eq!(number, 3, "three weeks after the start is microcycle three");
    assert_eq!(position.as_u8(), 2, "Sunday is the second ride of the week");
    assert_eq!(
        ride.published_session(),
        3,
        "and it is the published third session, which is not what it is called",
    );
    assert_eq!(
        ride.session().ride().duration().as_seconds(),
        2400,
        "Sunday takes the longer ride",
    );
}

/// **The week counts its own sessions and remembers the published ones.** The
/// operator rides two of the three a microcycle states, and those two are his
/// first and second — reporting the second as "session 3" is what he caught on
/// 2026-09-05.
#[test]
fn a_week_counts_its_own_sessions_and_keeps_the_published_numbers() {
    let programme = programme().expect("the fixture programme is valid");
    let week = programme.microcycle(1).expect("the first microcycle");

    assert_eq!(week.session_count(), 2, "two rides, so two of two");
    let ours: Vec<u8> = week.rides().keys().map(|at| at.as_u8()).collect();
    assert_eq!(ours, vec![1, 2], "numbered from one, in the order ridden");
    let published: Vec<u32> = week
        .rides()
        .values()
        .map(PlannedRide::published_session)
        .collect();
    assert_eq!(
        published,
        vec![1, 3],
        "taken from the published first and third"
    );
}

/// A day the programme does not ride and a day it does not cover are different
/// answers to different questions, and both are `None` rather than a guess.
#[test]
fn a_day_outside_the_programme_rides_nothing() {
    let programme = programme().expect("the fixture programme is valid");
    assert_eq!(programme.on(date(2026, 9, 25)), None, "Friday is the gym's");
    assert_eq!(
        programme.microcycle_of(date(2026, 9, 20)),
        None,
        "the day before it starts",
    );
    assert_eq!(
        programme.microcycle_of(date(2026, 10, 19)),
        None,
        "the Monday after the last microcycle ends",
    );
}

/// Monday, and the next ride is the Wednesday of that week rather than the
/// first Wednesday of the programme.
#[test]
fn the_next_riding_day_is_found_from_any_date() {
    let programme = programme().expect("the fixture programme is valid");
    assert_eq!(
        programme.next_riding_day(date(2026, 9, 28)),
        Some(date(2026, 9, 30)),
    );
    assert_eq!(
        programme.next_riding_day(date(2026, 9, 1)),
        Some(date(2026, 9, 23)),
        "a date before the start finds the programme's own first ride",
    );
}

/// The one thing the parts cannot guarantee between them. A weekday map naming
/// a session no microcycle holds would author a Wednesday nothing answers for.
#[test]
fn a_weekday_riding_a_session_no_microcycle_holds_is_refused() {
    let microcycles = NonEmpty::new(vec![microcycle(1).expect("a microcycle")])
        .expect("one microcycle is enough");
    let weekdays = CyclingWeekdays::new(vec![(
        Weekday::Friday,
        SessionPosition::new(3).expect("session three"),
    )])
    .expect("one day is a week");

    let refused = CyclingMesocycle::new(
        ProgrammeName::try_from("cycling-1").expect("a name"),
        jiff::Timestamp::now(),
        date(2026, 9, 21),
        microcycles,
        weekdays,
    );
    assert!(
        refused.is_err(),
        "the microcycle holds a first and a second"
    );
}

/// Two weekdays riding one session would prescribe the same ride twice in a
/// week, and one weekday riding two sessions is a day that cannot answer.
#[test]
fn a_weekday_map_names_each_day_and_each_session_once() {
    let first = SessionPosition::new(1).expect("session one");
    let second = SessionPosition::new(2).expect("session two");

    assert!(
        CyclingWeekdays::new(vec![(Weekday::Wednesday, first), (Weekday::Sunday, first)]).is_err(),
        "one session, two days",
    );
    assert!(
        CyclingWeekdays::new(vec![
            (Weekday::Wednesday, first),
            (Weekday::Wednesday, second)
        ])
        .is_err(),
        "one day, two sessions",
    );
    assert!(CyclingWeekdays::new(Vec::new()).is_err(), "no day at all");
}

/// Versions of one programme sit on top of each other; two different ones may
/// not. The rule is the gym's, and the sets it is applied to are what differ —
/// a cycling programme never competes with the gym block beside it.
#[test]
fn a_cycling_programme_occupies_the_weeks_it_runs() {
    let programme = programme().expect("the fixture programme is valid");
    let window = programme.window();
    assert_eq!(window.calendar_weeks(), 4);
    assert!(
        window.covers(date(2026, 10, 18)),
        "the last day it occupies"
    );
    assert!(!window.covers(date(2026, 10, 19)), "the day after");
}

/// The operator rides five minutes of his own after the minute Peloton builds
/// in, and it is applied when a session is prescribed rather than stored.
#[test]
fn the_operator_s_own_cool_down_is_added_to_the_class_s() {
    let planned = ride(PowerZone::Two, 1800, "45 min Power Zone Endurance Ride", 1)
        .expect("the fixture ride is valid");
    let session = planned.session();
    let extended =
        session.with_extra_cool_down(PositiveDuration::from_seconds(300).expect("five minutes"));

    assert_eq!(
        extended.cool_down().map(PositiveDuration::as_seconds),
        Some(360),
        "his five minutes on top of the class's one",
    );
    assert_eq!(
        extended.total().as_seconds(),
        session.total().as_seconds() + 300,
    );
}

/// Absent and zero are different claims: the FTP test ships with no cool-down
/// section at all, so five minutes added to it is five minutes.
#[test]
fn a_session_with_no_cool_down_gains_exactly_what_is_added() {
    let test = CyclingSession::new(
        PositiveDuration::from_seconds(600).expect("ten minutes"),
        Ride::Effort(PositiveDuration::from_seconds(1200).expect("twenty minutes")),
        None,
    );
    let extended =
        test.with_extra_cool_down(PositiveDuration::from_seconds(300).expect("five minutes"));
    assert_eq!(
        extended.cool_down().map(PositiveDuration::as_seconds),
        Some(300),
        "absent plus five minutes is five minutes, not six",
    );
}

/// A zone is a share of FTP, so watts are derived and never stored.
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

#[test]
fn a_programme_splits_into_the_mesocycles_the_operator_names() {
    // His, 2026-09-05: base 1 is µ1-4, base 2 is µ5-8, build is µ1-5, peak 1 is
    // µ1-4, peak 2 is µ5-8. **None of that is given to the code** — it falls out
    // of taking the shortest prefix that ends at its bottom level, and repeating.
    assert_eq!(partition(&BASE), vec![0..4, 4..8]);
    assert_eq!(partition(&PEAK), vec![0..4, 4..8]);
    assert_eq!(
        partition(&BUILD),
        vec![0..5],
        "Build is one mesocycle of five, not four and a spare"
    );
}

#[test]
fn a_programme_with_no_deload_splits_into_nothing() {
    // Four rising microcycles are a progression, not a mesocycle, and forcing a
    // split would invent a deload the programme does not contain.
    assert!(partition(&[100.0, 110.0, 120.0, 130.0]).is_empty());
    assert!(
        partition(&[0.0; 8]).is_empty(),
        "and neither is a flat programme, which has no working microcycle"
    );
}

#[test]
fn a_tail_that_is_no_mesocycle_is_left_out_rather_than_forced() {
    // Build's five, then two rising microcycles going nowhere.
    let trailing = [108.0, 123.0, 129.0, 141.0, 63.0, 100.0, 120.0];

    assert_eq!(
        partition(&trailing),
        vec![0..5],
        "the mesocycle is found and the tail is not made into one"
    );
}

/// **A programme longer than a month still counts weeks.** `jiff`'s date
/// difference is in days here, and a span expressed in months and days would
/// make every microcycle past the fourth resolve to the wrong week — so the
/// arithmetic is pinned rather than assumed.
#[test]
fn a_date_months_after_the_start_resolves_to_the_right_microcycle() {
    let microcycles = NonEmpty::new(
        (1..=13)
            .map(microcycle)
            .collect::<Result<Vec<_>, _>>()
            .expect("thirteen microcycles are valid"),
    )
    .expect("thirteen is non-empty");
    let weekdays = CyclingWeekdays::new(vec![(
        Weekday::Wednesday,
        SessionPosition::new(1).expect("session one"),
    )])
    .expect("one day is a week");
    let long = CyclingMesocycle::new(
        ProgrammeName::try_from("thirteen").expect("a name"),
        jiff::Timestamp::now(),
        date(2026, 9, 21),
        microcycles,
        weekdays,
    )
    .expect("the programme is valid");

    // Fifteen weeks and a day after the Monday it starts, the last microcycle
    // has ended; twelve weeks in, it is the thirteenth.
    assert_eq!(long.microcycle_of(date(2026, 12, 14)), Some(13));
    assert_eq!(long.microcycle_of(date(2026, 12, 20)), Some(13));
    assert_eq!(long.microcycle_of(date(2026, 12, 21)), None);
}
