//! A watch's own daily figures, one series per device (#254).
//!
//! The dates are the operator's: the Vivoactive 3's last recording on
//! 2024-12-02, the Forerunner 165's first on 2024-12-23, and two days of
//! resting heart rate between them that neither can be placed on.

use std::collections::{BTreeMap, BTreeSet};

use domain::{
    analytical::per_device::{Reading, Recorded, SentBy, per_device},
    device::{
        DeviceIdentifier, DeviceName, Measurand, Register, Role, SerialNumber, Sighting, Wear,
        register,
    },
    measure::BeatsPerMinute,
    normalised::{OperatorZone, StartedAt},
};
use jiff::civil::{Date, date};

type Failure = Box<dyn std::error::Error>;

fn sighting(serial: &str, name: &str, on: Date, worn: bool) -> Result<Sighting, Failure> {
    let zone = OperatorZone::try_from("Europe/London")?;
    let at = on.at(9, 0, 0, 0).to_zoned(zone.as_time_zone())?;
    Ok(Sighting {
        device: DeviceIdentifier::Serial(SerialNumber::try_from(serial.to_owned())?),
        also: Vec::new(),
        name: DeviceName::try_from(name.to_owned())?,
        role: Role::Recorder,
        measured: BTreeSet::from([Measurand::HeartRate]),
        sent: BTreeSet::new(),
        at: StartedAt::new(at.timestamp(), zone),
        source: "Garmin",
        worn,
    })
}

/// Two watches, one after the other, and a bike computer that is not worn.
fn devices(wear: Wear) -> Result<Register, Failure> {
    Ok(register(
        &[
            sighting("3960966298", "Vivoactive 3", date(2018, 1, 6), true)?,
            sighting("3960966298", "Vivoactive 3", date(2024, 12, 2), true)?,
            sighting("3442801430", "Edge 530", date(2023, 6, 27), false)?,
            sighting("3442801430", "Edge 530", date(2026, 4, 19), false)?,
            sighting("3483897389", "Forerunner 165", date(2024, 12, 23), true)?,
            sighting("3483897389", "Forerunner 165", date(2026, 10, 12), true)?,
        ],
        wear,
    ))
}

fn reading(on: Date, beats: u32) -> Result<Recorded<BeatsPerMinute>, Failure> {
    Ok(Recorded {
        reading: Reading {
            on,
            value: BeatsPerMinute::new(beats)?,
        },
        by: None,
    })
}

/// A reading whose own record names the watch.
fn named_by(on: Date, beats: u32, serial: &str) -> Result<Recorded<BeatsPerMinute>, Failure> {
    Ok(Recorded {
        by: Some(DeviceIdentifier::Serial(SerialNumber::try_from(
            serial.to_owned(),
        )?)),
        ..reading(on, beats)?
    })
}

fn named(sent_by: &SentBy) -> String {
    match sent_by {
        SentBy::Device(device) => device.name().to_string(),
        SentBy::Unknown => "unknown".to_owned(),
    }
}

#[test]
fn each_day_is_the_watch_worn_that_day_and_a_day_neither_was_is_its_own_series() {
    let register = devices(Wear::default()).expect("a register");
    let readings = vec![
        reading(date(2024, 12, 23), 51).expect("a reading"),
        reading(date(2024, 12, 21), 63).expect("a reading"),
        reading(date(2019, 3, 2), 48).expect("a reading"),
        reading(date(2024, 12, 2), 52).expect("a reading"),
        reading(date(2024, 12, 22), 53).expect("a reading"),
    ];

    let series = per_device(readings, Measurand::HeartRate, &register);

    let shape: Vec<(String, Vec<Date>)> = series
        .iter()
        .map(|one| {
            (
                named(&one.sent_by),
                one.readings.iter().map(|reading| reading.on).collect(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            (
                "Vivoactive 3".to_owned(),
                vec![date(2019, 3, 2), date(2024, 12, 2)]
            ),
            ("Forerunner 165".to_owned(), vec![date(2024, 12, 23)]),
            (
                "unknown".to_owned(),
                vec![date(2024, 12, 21), date(2024, 12, 22)]
            ),
        ]
    );
}

#[test]
fn a_watch_not_worn_that_day_did_not_take_the_reading() {
    let register =
        devices(Wear::new(BTreeMap::from([(date(2019, 3, 2), false)]))).expect("a register");

    let series = per_device(
        vec![reading(date(2019, 3, 2), 48).expect("a reading")],
        Measurand::HeartRate,
        &register,
    );

    let [only] = series.as_slice() else {
        panic!("{} series, not one", series.len());
    };
    assert_eq!(only.sent_by, SentBy::Unknown);
}

/// #569: a night whose own record names the watch is that watch's, after its
/// last activity as much as before it, and whatever the register would say.
#[test]
fn a_reading_its_own_record_names_is_that_device_s() {
    let register = devices(Wear::default()).expect("a register");

    let series = per_device(
        vec![
            named_by(date(2026, 10, 20), 50, "3483897389").expect("a reading"),
            named_by(date(2024, 12, 22), 53, "3483897389").expect("a reading"),
            named_by(date(2024, 10, 19), 55, "3498069814").expect("a reading"),
        ],
        Measurand::HeartRate,
        &register,
    );

    let shape: Vec<(String, Vec<Date>)> = series
        .iter()
        .map(|one| {
            (
                named(&one.sent_by),
                one.readings.iter().map(|reading| reading.on).collect(),
            )
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            ("serial/3498069814".to_owned(), vec![date(2024, 10, 19)]),
            (
                "Forerunner 165".to_owned(),
                vec![date(2024, 12, 22), date(2026, 10, 20)]
            ),
        ]
    );
}
