//! A main ride's heart rate, and the device that sent it (#152, #569).

use std::collections::BTreeSet;

use domain::{
    analytical::heart_rate::{RideHeartRate, SentBy, ride_heart_rate},
    canonical::{Attributed, NormalisedSessionId},
    cycling::{
        Cadence, CanonicalBikePlusRide, CanonicalCyclingSession, CanonicalRideRecord, Ftp,
        FtpProvenance, Resistance, RideSample, RideVenue, Speed, Watts,
    },
    device::{
        AntDeviceNumber, DeviceIdentifier, DeviceName, Measurand, Register, Role, SerialNumber,
        Sighting, Wear, register,
    },
    measure::{BeatsPerMinute, Duration, HeartRateSample, HeartRateSeries, Metres},
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
    Ok(ride_heart_rate(session, Some(ftp()?), register).ok_or("a main ride")?)
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
        ride_heart_rate(&session, Some(ftp().expect("an FTP")), &nobody()),
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
