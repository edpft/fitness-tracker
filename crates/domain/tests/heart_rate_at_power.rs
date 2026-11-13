//! Heart rate at a fixed power (#621, #688): every ride's heart rate against
//! its power over the last 5 minutes, read at 140 W and 20 minutes in, and the
//! mean of the last 42 days' rides read against the 42 days before.

use std::collections::BTreeSet;

use domain::{
    analytical::{
        body_weight::Direction,
        heart_rate::{RideHeartRate, SentBy, ride_heart_rate},
        heart_rate_at_power::{AtPower, Second, seconds, trended},
    },
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
use jiff::{
    ToSpan as _,
    civil::{Date, date},
};

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

const FIRST: Date = date(2026, 8, 1);

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
        date(2026, 7, 22),
        FtpProvenance::Estimated,
    )?)
}

/// A ride on `on` with a power and, where given, a heart rate for every second
/// from 0.
fn ride(on: Date, power: &[u32], beats: Option<&[u32]>) -> Fallible<CanonicalBikePlusRide> {
    let id = NormalisedSessionId::try_from(1)?;
    let samples = (0_u64..)
        .zip(power)
        .map(|(at, &watts)| {
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
            let samples = (0_u64..)
                .zip(beats)
                .map(|(at, &bpm)| {
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
        started_at: Attributed::new(started(on, 19)?, id),
        at: Attributed::new(RideVenue::new("class", "45 min Power Zone Ride")?, id),
        duration: Attributed::new(Duration::from_seconds(u64::try_from(power.len())?), id),
        distance: Attributed::new(Metres::from_millimetres(1), id),
        average_power: Attributed::new(Watts::from_u32(140), id),
        samples: Attributed::new(NonEmpty::new(samples)?, id),
        heart_rate,
        heart_rate_declared_missing: None,
        heart_rate_device: None,
    }))
}

fn watch(on: Date) -> Fallible<Sighting> {
    Ok(Sighting {
        device: DeviceIdentifier::Serial(SerialNumber::try_from("3483897389".to_owned())?),
        also: AntDeviceNumber::of_serial(3_483_897_389)
            .map(DeviceIdentifier::Ant)
            .into_iter()
            .collect(),
        name: DeviceName::try_from("Forerunner 165")?,
        role: Role::Recorder,
        measured: BTreeSet::from([Measurand::HeartRate]),
        sent: BTreeSet::from([Measurand::HeartRate]),
        at: started(on, 12)?,
        source: "Garmin",
        worn: false,
    })
}

/// The Forerunner 165, in use from before the first ride to after the last.
fn forerunner() -> Fallible<Register> {
    Ok(register(
        &[watch(date(2026, 7, 1))?, watch(date(2026, 12, 31))?],
        Wear::default(),
    ))
}

/// A ride's heart rate per zone and at power, through the domain's own entry.
fn read(ride: CanonicalBikePlusRide, register: &Register) -> Fallible<RideHeartRate> {
    let session = CanonicalCyclingSession::Ride {
        main: ride,
        cool_down: None,
    };
    Ok(ride_heart_rate(&session, Some(ftp()?), register).ok_or("a main ride")?)
}

/// A ride on `on` whose seconds lie, to the beat, on
/// `level + 0.4 × (watts − 140) + 0.25 × (minutes − 20)`, at two powers.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a heart rate between 100 and 160, rounded"
)]
fn modelled(on: Date, level: f64) -> Fallible<RideHeartRate> {
    let mut ridden = read(ride(on, &[140; 2], Some(&[120; 2]))?, &forerunner()?)?;
    let seconds = (0_u32..2_400)
        .step_by(5)
        .map(|at| {
            let watts = if (at / 300) % 2 == 0 { 120 } else { 160 };
            let bpm = 0.25f64.mul_add(
                f64::from(at) / 60.0 - 20.0,
                0.4f64.mul_add(f64::from(watts) - 140.0, level),
            );
            Second {
                deciwatts: watts * 10,
                at,
                beats: bpm.round() as u32,
            }
        })
        .collect();
    ridden.at_power = AtPower::Seconds(seconds);
    Ok(ridden)
}

#[test]
fn a_ride_whose_heart_rate_never_changes_is_a_stuck_sensor() {
    let power = vec![140; 900];
    let stuck = ride(FIRST, &power, Some(&vec![83; 900])).expect("a ride");

    assert_eq!(
        seconds(&stuck, Some(ftp().expect("an FTP"))),
        AtPower::Constant
    );
}

#[test]
fn a_ride_gives_nothing_without_heart_rate_an_ftp_or_a_second_in_range() {
    let ftp = ftp().expect("an FTP");
    let beats: Vec<u32> = (0..900).map(|at| 120 + at % 7).collect();

    let unrecorded = ride(FIRST, &[140; 900], None).expect("a ride");
    assert_eq!(seconds(&unrecorded, Some(ftp)), AtPower::Nothing);

    let untested = ride(FIRST, &[140; 900], Some(&beats)).expect("a ride");
    assert_eq!(
        seconds(&untested, None),
        AtPower::Nothing,
        "no zones without an FTP"
    );

    let easy = ride(FIRST, &[80; 900], Some(&beats)).expect("a ride");
    assert_eq!(
        seconds(&easy, Some(ftp)),
        AtPower::Nothing,
        "40% of FTP is below what is read"
    );

    let hard = ride(FIRST, &[220; 900], Some(&beats)).expect("a ride");
    assert_eq!(
        seconds(&hard, Some(ftp)),
        AtPower::Nothing,
        "110% of FTP is above it"
    );
}

#[test]
fn power_is_weighted_over_the_last_five_minutes_with_the_latest_seconds_heaviest() {
    let power: Vec<u32> = (0..1_200)
        .map(|at| if at < 600 { 100 } else { 160 })
        .collect();
    let beats: Vec<u32> = (0..1_200).map(|at| 110 + at % 3).collect();
    let AtPower::Seconds(read) = seconds(
        &ride(FIRST, &power, Some(&beats)).expect("a ride"),
        Some(ftp().expect("an FTP")),
    ) else {
        panic!("seconds read");
    };
    let at = |second: u32| {
        read.iter()
            .find(|read| read.at == second)
            .map(|read| read.deciwatts)
    };

    assert_eq!(
        at(300),
        Some(1_000),
        "the first second read has five minutes behind it"
    );
    assert_eq!(at(595), Some(1_000), "before the step");
    let after = at(690).expect("90 s after the step");
    assert!(
        (1_350..1_420).contains(&after),
        "one recuperation time after the step, about 63% of the way: {after}"
    );
    assert_eq!(
        at(905),
        Some(1_600),
        "five minutes after the step, all of it is the new power"
    );
    assert!(
        read.iter().all(|read| read.at % 5 == 0),
        "one second in five"
    );
}

#[test]
fn each_ride_is_read_at_140_watts_and_20_minutes_through_slopes_every_ride_shares() {
    let rides: Vec<RideHeartRate> = [(0, 140.0), (4, 138.0), (8, 141.0), (12, 137.0)]
        .into_iter()
        .map(|(day, level)| modelled(FIRST + day.days(), level))
        .collect::<Fallible<_>>()
        .expect("rides");

    let series = trended(&rides);
    let [watch] = series.as_slice() else {
        panic!("one device: {series:?}");
    };
    assert!(
        (watch.per_watt - 0.4).abs() < 0.01,
        "per watt: {}",
        watch.per_watt
    );
    assert!(
        (watch.per_minute - 0.25).abs() < 0.01,
        "per minute: {}",
        watch.per_minute
    );
    let read: Vec<f64> = watch.rides.iter().map(|ride| ride.bpm).collect();
    for (read, level) in read.iter().zip([140.0, 138.0, 141.0, 137.0]) {
        assert!((read - level).abs() < 0.5, "{read} against {level}");
    }
}

#[test]
fn a_ride_whose_device_is_not_known_is_in_no_series() {
    let mut ride = modelled(FIRST, 140.0).expect("a ride");
    ride.sent_by = SentBy::Unknown;

    assert_eq!(trended(&[ride]), Vec::new());
}

#[test]
fn the_last_42_days_are_read_against_the_42_before_beyond_the_noise_of_the_two() {
    // Every fourth day for twelve weeks: 140 bpm, alternating a beat either
    // side, then 130 for the last six weeks.
    let rides: Vec<RideHeartRate> = (0..21)
        .map(|n| {
            let wobble = if n % 2 == 0 { 1.0 } else { -1.0 };
            let level = if n < 10 { 140.0 } else { 130.0 } + wobble;
            modelled(FIRST + (4 * n).days(), level)
        })
        .collect::<Fallible<_>>()
        .expect("rides");

    let series = trended(&rides);
    let [watch] = series.as_slice() else {
        panic!("one device");
    };
    let noise = watch.noise.expect("enough successive rides");
    assert!(
        noise < 3.0,
        "the noise is successive rides' scatter: {noise}"
    );
    let change = watch.change().expect("both windows hold three rides");
    assert!(
        (change.by + 10.0).abs() < 1.0,
        "fell ten beats: {}",
        change.by
    );
    assert_eq!(change.direction, Some(Direction::Falling));
}

#[test]
fn fewer_than_three_rides_in_42_days_is_no_line_and_not_known_yet() {
    let rides: Vec<RideHeartRate> = [0, 30, 75, 76]
        .into_iter()
        .map(|day| modelled(FIRST + day.days(), 140.0))
        .collect::<Fallible<_>>()
        .expect("rides");

    let series = trended(&rides);
    let [watch] = series.as_slice() else {
        panic!("one device");
    };
    assert!(watch.line.is_empty(), "no window holds three rides");
    assert_eq!(watch.change(), None);
}
