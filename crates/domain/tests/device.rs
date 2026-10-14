//! Which device sent a Bike+ ride's heart rate (#427).
//!
//! The serial numbers and dates are the operator's, read off his Garmin
//! recordings and Peloton's account on 2026-10-13.

use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU16,
};

use domain::{
    device::{
        AntDeviceNumber, Device, DeviceIdentifier, DeviceName, InUse, Measurand, Register, Role,
        SerialNumber, Sighting, Wear, register,
    },
    normalised::{OperatorZone, StartedAt},
};
use jiff::civil::{Date, date};

const FORERUNNER_165: u32 = 3_483_897_389;
const VIVOACTIVE_3: u32 = 3_960_966_298;
const TICKR: u32 = 3_339_528_293;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

/// A sighting as a Garmin recording gives one: the serial, and the ANT+
/// number it implies.
fn garmin(
    serial: u32,
    name: &str,
    role: Role,
    measured: &[Measurand],
    sent: &[Measurand],
    on: Date,
) -> Fallible<Sighting> {
    let zone = OperatorZone::try_from("Europe/London".to_owned())?;
    let at = on
        .at(12, 0, 0, 0)
        .to_zoned(zone.as_time_zone())?
        .timestamp();
    Ok(Sighting {
        device: DeviceIdentifier::Serial(SerialNumber::try_from(serial.to_string())?),
        also: AntDeviceNumber::of_serial(serial)
            .map(DeviceIdentifier::Ant)
            .into_iter()
            .collect(),
        name: DeviceName::try_from(name)?,
        role,
        measured: measured.iter().copied().collect(),
        sent: sent.iter().copied().collect(),
        at: StartedAt::new(at, zone),
        source: "Garmin",
        worn: role == Role::Recorder && sent.contains(&Measurand::HeartRate),
    })
}

/// The monitor Peloton's account says the Bike+ is paired with, by its ANT+
/// number alone.
fn peloton(number: u16, on: Date) -> Fallible<Sighting> {
    let zone = OperatorZone::try_from("Europe/London".to_owned())?;
    let at = on
        .at(20, 0, 0, 0)
        .to_zoned(zone.as_time_zone())?
        .timestamp();
    Ok(Sighting {
        device: DeviceIdentifier::Ant(AntDeviceNumber::new(
            NonZeroU16::new(number).ok_or("a number")?,
        )),
        also: Vec::new(),
        name: DeviceName::try_from(format!("Device: {number}"))?,
        role: Role::Paired,
        measured: BTreeSet::from([Measurand::HeartRate]),
        sent: BTreeSet::from([Measurand::HeartRate]),
        at: StartedAt::new(at, zone),
        source: "Peloton",
        worn: false,
    })
}

/// What the record shows of the watches and the strap the Bike+ rides were
/// ridden with, at the ends of each one's period. Both watches broadcast heart
/// rate, which is a fact about the model.
fn record() -> Fallible<Vec<Sighting>> {
    use Measurand::HeartRate;
    use Role::{Paired, Recorder};
    Ok(vec![
        garmin(
            VIVOACTIVE_3,
            "Vivoactive 3",
            Recorder,
            &[HeartRate],
            &[HeartRate],
            date(2018, 1, 6),
        )?,
        garmin(
            VIVOACTIVE_3,
            "Vivoactive 3",
            Recorder,
            &[],
            &[HeartRate],
            date(2024, 11, 22),
        )?,
        garmin(
            TICKR,
            "Wahoo heart rate sensor",
            Paired,
            &[HeartRate],
            &[HeartRate],
            date(2021, 7, 17),
        )?,
        garmin(
            TICKR,
            "Wahoo heart rate sensor",
            Paired,
            &[HeartRate],
            &[HeartRate],
            date(2024, 6, 30),
        )?,
        garmin(
            FORERUNNER_165,
            "Forerunner 165",
            Recorder,
            &[HeartRate],
            &[HeartRate],
            date(2024, 12, 23),
        )?,
        garmin(
            FORERUNNER_165,
            "Forerunner 165",
            Recorder,
            &[HeartRate],
            &[HeartRate],
            date(2026, 9, 29),
        )?,
    ])
}

fn named<'a>(found: &InUse<'a>) -> Option<&'a str> {
    match found {
        InUse::One(device) => Some(device.name().as_str()),
        InUse::Several(_) | InUse::Unknown => None,
    }
}

fn find(register: &Register, serial: u32) -> Option<&Device> {
    let identifier = DeviceIdentifier::Serial(SerialNumber::try_from(serial.to_string()).ok()?);
    register
        .devices()
        .iter()
        .find(|device| device.is(&identifier))
}

/// The three stretches of the operator's Bike+ rides: the Vivoactive 3 alone,
/// the Forerunner 165 alone, and the TICKR beside the Vivoactive 3, where
/// nothing records which one the bike was paired with.
#[test]
fn each_stretch_of_rides_goes_to_the_device_in_use_then() {
    let devices = register(&record().expect("the record"), Wear::default());

    assert_eq!(
        named(&devices.sent_by(Measurand::HeartRate, date(2024, 8, 1))),
        Some("Vivoactive 3")
    );
    assert_eq!(
        named(&devices.sent_by(Measurand::HeartRate, date(2025, 6, 1))),
        Some("Forerunner 165")
    );
    let InUse::Several(candidates) = devices.sent_by(Measurand::HeartRate, date(2024, 2, 1)) else {
        panic!("the strap and the watch were both in use");
    };
    assert_eq!(candidates.len(), 2);
}

/// Peloton's account names the monitor 3629, the low sixteen bits of the
/// Forerunner 165's serial: one device, in use the day the account was read,
/// so a ride after the watch's last Garmin activity is still the watch's.
#[test]
fn the_monitor_the_bike_is_paired_with_is_the_watch_with_that_number() {
    let before = register(&record().expect("the record"), Wear::default());
    assert_eq!(
        before.sent_by(Measurand::HeartRate, date(2026, 10, 7)),
        InUse::Unknown
    );

    let mut sightings = record().expect("the record");
    sightings.push(peloton(3629, date(2026, 10, 13)).expect("a sighting"));
    let after = register(&sightings, Wear::default());

    assert_eq!(after.devices().len(), before.devices().len());
    let watch = find(&after, FORERUNNER_165).expect("the watch");
    assert_eq!(watch.name().as_str(), "Forerunner 165");
    assert_eq!(watch.last_used(), date(2026, 10, 13));
    for ride in [date(2026, 9, 30), date(2026, 10, 4), date(2026, 10, 7)] {
        assert_eq!(
            named(&after.sent_by(Measurand::HeartRate, ride)),
            Some("Forerunner 165")
        );
    }
}

/// A device that measures heart rate for itself and sends it nowhere is never
/// a candidate, however long it was in use.
#[test]
fn a_device_that_sends_nothing_is_no_candidate() {
    let mut sightings = record().expect("the record");
    sightings.push(
        garmin(
            3_442_801_430,
            "Edge 530",
            Role::Recorder,
            &[Measurand::HeartRate],
            &[],
            date(2025, 6, 1),
        )
        .expect("a sighting"),
    );
    let devices = register(&sightings, Wear::default());

    assert_eq!(
        named(&devices.sent_by(Measurand::HeartRate, date(2025, 6, 1))),
        Some("Forerunner 165")
    );
}

/// A watch is called what it calls itself, whichever record named it first.
#[test]
fn a_device_is_named_as_the_recorder_it_was() {
    let mut sightings = vec![peloton(3629, date(2024, 12, 1)).expect("a sighting")];
    sightings.extend(record().expect("the record"));
    let devices = register(&sightings, Wear::default());

    let watch = find(&devices, FORERUNNER_165).expect("the watch");
    assert_eq!(watch.name().as_str(), "Forerunner 165");
    assert_eq!(watch.first_used(), date(2024, 12, 1));
}

/// A serial whose low sixteen bits are zero names no ANT+ device.
#[test]
fn a_serial_ending_in_sixteen_zero_bits_has_no_ant_number() {
    assert_eq!(AntDeviceNumber::of_serial(7_864_320), None);
    assert_eq!(
        AntDeviceNumber::of_serial(FORERUNNER_165).map(|number| number.get().get()),
        Some(3629)
    );
    assert_eq!(
        AntDeviceNumber::of_serial(TICKR).map(|number| number.get().get()),
        Some(10341)
    );
}

/// A day the account shows nothing worn takes the watch out, so the rides of
/// the operator's gap are the strap's alone.
#[test]
fn a_watch_not_worn_that_day_is_no_candidate() {
    let gapped = register(
        &record().expect("the record"),
        Wear::new(BTreeMap::from([
            (date(2024, 1, 15), false),
            (date(2024, 8, 1), true),
        ])),
    );

    assert_eq!(
        named(&gapped.sent_by(Measurand::HeartRate, date(2024, 1, 15))),
        Some("Wahoo heart rate sensor")
    );
    assert_eq!(
        named(&gapped.sent_by(Measurand::HeartRate, date(2024, 8, 1))),
        Some("Vivoactive 3")
    );
    // A day the account does not hold leaves the span standing.
    let InUse::Several(_) = gapped.sent_by(Measurand::HeartRate, date(2024, 2, 1)) else {
        panic!("both were in use, as far as anything says");
    };
}
