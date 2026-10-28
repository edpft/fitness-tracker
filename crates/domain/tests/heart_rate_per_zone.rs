//! Heart rate per power zone, for each main ride (#152).
//!
//! The method is the operator's, 2026-10-14: a 30-second mean of power, and a
//! second counts only once the ride has been in its zone for 120 seconds.
//!
//! Beside it, the class's Z3 intervals of at least 3 minutes with no harder
//! interval before them (operator, 2026-10-19, #415).

use std::collections::BTreeSet;

use domain::{
    analytical::heart_rate::{ByZone, RideHeartRate, SentBy, heart_rate_by_zone},
    canonical::{Attributed, NormalisedSessionId},
    cycling::{
        Cadence, CanonicalBikePlusRide, CanonicalCyclingSession, CanonicalRideRecord,
        CyclingSession, Ftp, FtpProvenance, Interval, PowerZone, Resistance, Ride, RideSample,
        RideVenue, Speed, Watts,
    },
    device::{
        AntDeviceNumber, DeviceIdentifier, DeviceName, Measurand, Register, Role, SerialNumber,
        Sighting, Wear, register,
    },
    measure::{
        BeatsPerMinute, Duration, HeartRateSample, HeartRateSeries, Metres, PositiveDuration,
    },
    normalised::{OperatorZone, StartedAt},
    sequence::NonEmpty,
};
use jiff::civil::{Date, date};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

const RIDE_DAY: Date = date(2026, 10, 7);

fn started(on: Date, hour: i8) -> Fallible<StartedAt> {
    let zone = OperatorZone::try_from("Europe/London".to_owned())?;
    let at = on
        .at(hour, 0, 0, 0)
        .to_zoned(zone.as_time_zone())?
        .timestamp();
    Ok(StartedAt::new(at, zone))
}

fn ftp() -> Fallible<Ftp> {
    Ok(Ftp::new(
        Watts::from_u32(200),
        date(2026, 9, 23),
        FtpProvenance::Estimated,
    )?)
}

/// A ride whose power and heart rate are given second by second, from 1.
fn ride(power: &[(u64, u32)], beats: Option<&[(u64, u32)]>) -> Fallible<CanonicalBikePlusRide> {
    ridden_with(power, beats, None)
}

/// As [`ride`], ridden with the monitor Peloton's account names.
fn ridden_with(
    power: &[(u64, u32)],
    beats: Option<&[(u64, u32)]>,
    monitor: Option<Sighting>,
) -> Fallible<CanonicalBikePlusRide> {
    let id = NormalisedSessionId::try_from(1)?;
    let samples = power
        .iter()
        .map(|&(at, watts)| {
            Ok(RideSample {
                at: Duration::from_seconds(at),
                power: Watts::from_u32(watts),
                cadence: Cadence::from_revolutions_per_minute(85),
                resistance: Resistance::from_percentage(40)?,
                speed: Speed::from_millimetres_per_hour(30_000_000),
            })
        })
        .collect::<Fallible<Vec<_>>>()?;
    let heart_rate = beats
        .map(|beats| {
            let samples = beats
                .iter()
                .map(|&(at, bpm)| {
                    Ok(HeartRateSample {
                        at: Duration::from_seconds(at),
                        beats_per_minute: BeatsPerMinute::new(bpm)?,
                    })
                })
                .collect::<Fallible<Vec<_>>>()?;
            Fallible::Ok(Attributed::new(
                HeartRateSeries::new(NonEmpty::new(samples)?),
                id,
            ))
        })
        .transpose()?;
    Ok(CanonicalBikePlusRide::new(CanonicalRideRecord {
        started_at: Attributed::new(started(RIDE_DAY, 19)?, id),
        at: Attributed::new(RideVenue::new("class", "45 min Power Zone Ride")?, id),
        duration: Attributed::new(Duration::from_seconds(u64::try_from(power.len())?), id),
        distance: Attributed::new(Metres::from_millimetres(1), id),
        average_power: Attributed::new(Watts::from_u32(150), id),
        samples: Attributed::new(NonEmpty::new(samples)?, id),
        heart_rate,
        heart_rate_declared_missing: None,
        heart_rate_device: monitor.map(|monitor| Attributed::new(monitor, id)),
    }))
}

const fn main_ride(ride: CanonicalBikePlusRide) -> CanonicalCyclingSession {
    CanonicalCyclingSession::Ride {
        main: ride,
        cool_down: None,
    }
}

/// Seconds `from..=to` at one power.
fn held(from: u64, to: u64, watts: u32) -> Vec<(u64, u32)> {
    (from..=to).map(|at| (at, watts)).collect()
}

fn nobody() -> Register {
    register(&[], Wear::default())
}

fn of(session: &CanonicalCyclingSession, register: &Register) -> Fallible<RideHeartRate> {
    Ok(heart_rate_by_zone(session, Some(ftp()?), None, register).ok_or("a main ride")?)
}

/// Every zone's average, lowest zone first.
fn averages(ride: &RideHeartRate) -> Vec<(PowerZone, u32, u64)> {
    match &ride.by_zone {
        ByZone::Zones(zones) => zones
            .iter()
            .map(|zone| (zone.zone, zone.average.as_u32(), zone.over.as_seconds()))
            .collect(),
        _ => Vec::new(),
    }
}

#[test]
fn a_band_s_upper_bound_is_in_it_and_anything_above_is_in_the_next() {
    let ftp = ftp().expect("an FTP");
    let zone = |watts| PowerZone::at(Watts::from_u32(watts), ftp);
    assert_eq!(zone(0), PowerZone::One);
    assert_eq!(zone(110), PowerZone::One, "55%");
    assert_eq!(zone(111), PowerZone::Two, "55.5%");
    assert_eq!(zone(150), PowerZone::Two, "75%");
    assert_eq!(zone(151), PowerZone::Three, "75.5%");
    assert_eq!(zone(300), PowerZone::Six, "150%");
    assert_eq!(zone(301), PowerZone::Seven, "150.5%");
}

#[test]
fn only_seconds_after_two_minutes_in_a_zone_count() {
    // Five minutes at zone two (140 W of 200). The heart rate is still rising
    // for the first two, at 100, and settled at 130 after.
    let power = held(1, 300, 140);
    let beats: Vec<(u64, u32)> = (1..=300)
        .map(|at| (at, if at <= 121 { 100 } else { 130 }))
        .collect();
    let session = main_ride(ride(&power, Some(&beats)).expect("a ride"));
    let ride = of(&session, &nobody()).expect("a ride's heart rate");
    // The zone is entered at second 1, so seconds 121 to 300 count: 180 of
    // them, the first at 100 and the rest at 130.
    assert_eq!(averages(&ride), vec![(PowerZone::Two, 130, 180)]);
}

#[test]
fn power_is_a_thirty_second_mean_so_a_spike_leaves_a_zone_while_it_is_in_the_window() {
    // Zone two throughout, but for ten seconds at 400 W in the middle. Two of
    // those seconds in the window take the mean past 150 W, out of zone two,
    // and the two minutes start again once the window is back under it.
    let mut power = held(1, 200, 140);
    power.extend(held(201, 210, 400));
    power.extend(held(211, 600, 140));
    let beats: Vec<(u64, u32)> = (1..=600).map(|at| (at, 120)).collect();
    let session = main_ride(ride(&power, Some(&beats)).expect("a ride"));
    let ride = of(&session, &nobody()).expect("a ride's heart rate");
    // Zone two from 1 to 201 counts 121 to 201; it is entered again at 239,
    // when only the spike's last second is in the window, and counts 359 to
    // 600. Nothing else is held for two minutes.
    assert_eq!(averages(&ride), vec![(PowerZone::Two, 120, 81 + 242)]);
}

#[test]
fn a_skipped_second_does_not_restart_the_time_in_a_zone() {
    // Peloton skips single seconds throughout a ride. Here every tenth one.
    let power: Vec<(u64, u32)> = (1..=300)
        .filter(|at| at % 10 != 0)
        .map(|at| (at, 140))
        .collect();
    let beats: Vec<(u64, u32)> = power.iter().map(|&(at, _)| (at, 125)).collect();
    let session = main_ride(ride(&power, Some(&beats)).expect("a ride"));
    let ride = of(&session, &nobody()).expect("a ride's heart rate");
    assert_eq!(
        averages(&ride),
        vec![(PowerZone::Two, 125, 162)],
        "121 to 300 less the eighteen skipped"
    );
}

#[test]
fn a_ride_with_no_heart_rate_says_so() {
    let session = main_ride(ride(&held(1, 300, 140), None).expect("a ride"));
    let ride = of(&session, &nobody()).expect("a ride's heart rate");
    assert_eq!(ride.by_zone, ByZone::NoHeartRate);
}

#[test]
fn a_ride_before_any_ftp_has_no_zones() {
    let beats: Vec<(u64, u32)> = (1..=300).map(|at| (at, 120)).collect();
    let session = main_ride(ride(&held(1, 300, 140), Some(&beats)).expect("a ride"));
    let ride = heart_rate_by_zone(&session, None, None, &nobody()).expect("a main ride");
    assert_eq!(ride.by_zone, ByZone::NoFtp);
}

#[test]
fn a_ride_that_holds_no_zone_for_two_minutes_says_so() {
    // Ninety seconds in each of two zones, alternating.
    let power: Vec<(u64, u32)> = (1..=600)
        .map(|at| (at, if (at / 90) % 2 == 0 { 140 } else { 250 }))
        .collect();
    let beats: Vec<(u64, u32)> = (1..=600).map(|at| (at, 120)).collect();
    let session = main_ride(ride(&power, Some(&beats)).expect("a ride"));
    let ride = of(&session, &nobody()).expect("a ride's heart rate");
    assert_eq!(ride.by_zone, ByZone::NoZoneHeld);
}

#[test]
fn a_test_is_not_a_main_ride() {
    let beats: Vec<(u64, u32)> = (1..=300).map(|at| (at, 150)).collect();
    let session = CanonicalCyclingSession::Test {
        warm_up: ride(&held(1, 300, 140), Some(&beats)).expect("a ride"),
        effort: ride(&held(1, 300, 210), Some(&beats)).expect("a ride"),
        cool_down: None,
    };
    assert_eq!(
        heart_rate_by_zone(&session, Some(ftp().expect("an FTP")), None, &nobody()),
        None
    );
}

/// A watch recording on `on`, which broadcasts heart rate.
fn watch(serial: u32, name: &str, on: Date) -> Fallible<Sighting> {
    Ok(Sighting {
        device: DeviceIdentifier::Serial(SerialNumber::try_from(serial.to_string())?),
        also: AntDeviceNumber::of_serial(serial)
            .map(DeviceIdentifier::Ant)
            .into_iter()
            .collect(),
        name: DeviceName::try_from(name)?,
        role: Role::Recorder,
        measured: BTreeSet::from([Measurand::HeartRate]),
        sent: BTreeSet::from([Measurand::HeartRate]),
        at: started(on, 12)?,
        source: "Garmin",
        worn: false,
    })
}

#[test]
fn a_ride_carries_the_one_device_in_use_and_otherwise_none() {
    let beats: Vec<(u64, u32)> = (1..=300).map(|at| (at, 120)).collect();
    let session = main_ride(ride(&held(1, 300, 140), Some(&beats)).expect("a ride"));

    let one = register(
        &[
            watch(3_483_897_389, "Forerunner 165", date(2026, 9, 1)).expect("a sighting"),
            watch(3_483_897_389, "Forerunner 165", date(2026, 10, 31)).expect("a sighting"),
        ],
        Wear::default(),
    );
    let SentBy::Device(device) = of(&session, &one).expect("a ride's heart rate").sent_by else {
        panic!("the Forerunner 165 was the only device in use");
    };
    assert_eq!(device.name().as_str(), "Forerunner 165");

    let two = register(
        &[
            watch(3_483_897_389, "Forerunner 165", date(2026, 9, 1)).expect("a sighting"),
            watch(3_483_897_389, "Forerunner 165", date(2026, 10, 31)).expect("a sighting"),
            watch(3_960_966_298, "Vivoactive 3", date(2026, 9, 1)).expect("a sighting"),
            watch(3_960_966_298, "Vivoactive 3", date(2026, 10, 31)).expect("a sighting"),
        ],
        Wear::default(),
    );
    assert_eq!(
        of(&session, &two).expect("a ride's heart rate").sent_by,
        SentBy::Unknown,
        "two in use is not known"
    );
    assert_eq!(
        of(&session, &nobody())
            .expect("a ride's heart rate")
            .sent_by,
        SentBy::Unknown
    );
}

/// The monitor Peloton's account names: ANT+ 3629.
fn paired_monitor() -> Fallible<Sighting> {
    Ok(Sighting {
        device: DeviceIdentifier::Ant(AntDeviceNumber::of_serial(3629).ok_or("an ANT+ number")?),
        also: Vec::new(),
        name: DeviceName::try_from("Device: 3629")?,
        role: Role::Paired,
        measured: BTreeSet::from([Measurand::HeartRate]),
        sent: BTreeSet::from([Measurand::HeartRate]),
        at: started(RIDE_DAY, 19)?,
        source: "Peloton",
        worn: false,
    })
}

/// #569: a ride Peloton's account names a monitor for is placed on it, under
/// the name of the device the register knows by that number.
#[test]
fn a_ride_ridden_with_a_named_monitor_is_placed_on_it() {
    let beats: Vec<(u64, u32)> = (1..=300).map(|at| (at, 120)).collect();
    let session = main_ride(
        ridden_with(
            &held(1, 300, 140),
            Some(&beats),
            Some(paired_monitor().expect("a monitor")),
        )
        .expect("a ride"),
    );

    let two = register(
        &[
            watch(3_483_897_389, "Forerunner 165", date(2026, 9, 1)).expect("a sighting"),
            watch(3_483_897_389, "Forerunner 165", date(2026, 10, 31)).expect("a sighting"),
            watch(3_960_966_298, "Vivoactive 3", date(2026, 9, 1)).expect("a sighting"),
            watch(3_960_966_298, "Vivoactive 3", date(2026, 10, 31)).expect("a sighting"),
        ],
        Wear::default(),
    );
    let SentBy::Device(device) = of(&session, &two).expect("a ride's heart rate").sent_by else {
        panic!("the account names the monitor, so two watches in use is no question");
    };
    assert_eq!(device.name().as_str(), "Forerunner 165");

    let SentBy::Device(device) = of(&session, &nobody())
        .expect("a ride's heart rate")
        .sent_by
    else {
        panic!("a monitor the register does not hold is still the one named");
    };
    assert_eq!(device.name().as_str(), "Device: 3629");
}

/// A class: `warm_up` seconds, then each interval in turn.
fn class(warm_up: u64, intervals: &[(PowerZone, u64)]) -> Fallible<CyclingSession> {
    let intervals = intervals
        .iter()
        .map(|&(zone, seconds)| {
            Ok(Interval::new(
                zone,
                PositiveDuration::from_seconds(seconds)?,
            ))
        })
        .collect::<Fallible<Vec<_>>>()?;
    Ok(CyclingSession::new(
        PositiveDuration::from_seconds(warm_up)?,
        Ride::Intervals(NonEmpty::new(intervals)?),
        None,
    ))
}

/// The heart rate over a class's Z3 intervals, for a ride whose heart rate
/// is `beats` at every second.
fn in_intervals(
    prescribed: &CyclingSession,
    beats: impl Fn(u64) -> u32,
) -> Fallible<Option<(u32, u64, usize)>> {
    let power = held(0, 2_000, 140);
    let beats: Vec<(u64, u32)> = (0..=2_000).map(|at| (at, beats(at))).collect();
    let session = main_ride(ride(&power, Some(&beats))?);
    let read = heart_rate_by_zone(&session, Some(ftp()?), Some(prescribed), &nobody())
        .ok_or("a main ride")?
        .in_intervals;
    Ok(read.map(|read| {
        (
            read.average.as_u32(),
            read.over.as_seconds(),
            read.intervals,
        )
    }))
}

#[test]
fn a_z3_interval_is_read_after_its_first_two_minutes_on_the_class_s_clock() {
    // 600 s of warm-up, 300 s at Z2, then 240 s at Z3 from second 900.
    let prescribed = class(
        600,
        &[
            (PowerZone::Two, 300),
            (PowerZone::Three, 240),
            (PowerZone::One, 120),
        ],
    )
    .expect("a class");
    // 150 until two minutes into the interval, and 140 after.
    let read = in_intervals(&prescribed, |at| if at < 1_020 { 150 } else { 140 }).expect("a ride");
    assert_eq!(read, Some((140, 120, 1)), "seconds 1020 to 1139");
}

#[test]
fn only_z3_intervals_of_three_minutes_with_none_harder_before_are_read() {
    let prescribed = class(
        600,
        &[
            // Too short.
            (PowerZone::Three, 179),
            (PowerZone::Two, 121),
            // Read: 900 to 1080, from 1020.
            (PowerZone::Three, 180),
            (PowerZone::Four, 120),
            // After a harder interval.
            (PowerZone::Three, 300),
        ],
    )
    .expect("a class");
    let read = in_intervals(&prescribed, |at| {
        if (1_020..1_080).contains(&at) {
            140
        } else {
            170
        }
    })
    .expect("a ride");
    assert_eq!(read, Some((140, 60, 1)));
}

#[test]
fn a_class_with_no_such_interval_reads_nothing() {
    let prescribed = class(600, &[(PowerZone::Two, 600), (PowerZone::Four, 300)]).expect("a class");
    assert_eq!(in_intervals(&prescribed, |_| 140).expect("a ride"), None);
}
