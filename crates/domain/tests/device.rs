//! The devices the operator uses, and when each was in use (#427).
//!
//! The dates are his, read off his Garmin recordings on 2026-10-13.

use std::collections::BTreeSet;

use domain::{
    device::{
        AddedDevice, AntDeviceNumber, Basis, Contradiction, DeviceIdentifier, DeviceName,
        DeviceStatement, InUse, IssuedId, LastUsed, Measurand, Platform, Role, SerialNumber,
        Sighting, Stated, StatedDevice, Statement, check, register, sent_by,
    },
    normalised::{OperatorZone, StartedAt},
};
use jiff::{
    Timestamp,
    civil::{Date, date},
};
use proptest::prelude::*;

const FORERUNNER_165: &str = "3483897389";
const VIVOACTIVE_3: &str = "3960966298";
const TICKR_ANT: &str = "3339528293";
const TICKR_BLUETOOTH: &str = "211011341";

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

fn sighting(
    serial: &str,
    name: &str,
    role: Role,
    measured: &[Measurand],
    on: Date,
) -> Fallible<Sighting> {
    let zone = OperatorZone::try_from("Europe/London".to_owned())?;
    let at = on
        .at(12, 0, 0, 0)
        .to_zoned(zone.as_time_zone())?
        .timestamp();
    let measured: BTreeSet<Measurand> = measured.iter().copied().collect();
    // As Garmin's recordings say it: a paired sensor sent what it measured to
    // whatever wrote the recording, and the recorder sent nothing.
    let sent = if role == Role::Paired {
        measured.clone()
    } else {
        BTreeSet::new()
    };
    let also = serial
        .parse::<u32>()
        .ok()
        .and_then(AntDeviceNumber::of_serial)
        .map(DeviceIdentifier::Ant)
        .into_iter()
        .collect();
    Ok(Sighting {
        device: DeviceIdentifier::Serial(SerialNumber::try_from(serial)?),
        also,
        name: DeviceName::try_from(name)?,
        role,
        measured,
        sent,
        at: StartedAt::new(at, zone),
        source: "Garmin",
    })
}

fn serial(text: &str) -> DeviceIdentifier {
    // Every serial these tests name is a valid one; the fallback is never read.
    SerialNumber::try_from(text).map_or(
        DeviceIdentifier::Stated(StatedDevice::new(std::num::NonZeroU64::MIN)),
        DeviceIdentifier::Serial,
    )
}

/// A statement made at eight in the evening of `on`, UTC.
fn stated(on: Date, statement: Statement) -> Fallible<Stated> {
    Ok(Stated {
        at: on
            .at(20, 0, 0, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)?
            .timestamp(),
        statement,
    })
}

/// What the record shows of the watches and the strap the Bike+ rides were
/// ridden with, at the ends of each one's period.
fn record() -> Fallible<Vec<Sighting>> {
    use Measurand::HeartRate;
    [
        sighting(
            VIVOACTIVE_3,
            "Vivoactive 3",
            Role::Recorder,
            &[HeartRate],
            date(2018, 1, 6),
        ),
        sighting(
            VIVOACTIVE_3,
            "Vivoactive 3",
            Role::Recorder,
            &[],
            date(2024, 12, 2),
        ),
        sighting(
            TICKR_ANT,
            "Wahoo heart rate sensor",
            Role::Paired,
            &[HeartRate],
            date(2021, 7, 17),
        ),
        sighting(
            TICKR_ANT,
            "Wahoo heart rate sensor",
            Role::Paired,
            &[HeartRate],
            date(2024, 6, 30),
        ),
        sighting(
            TICKR_BLUETOOTH,
            "heart rate sensor",
            Role::Paired,
            &[HeartRate],
            date(2023, 9, 29),
        ),
        sighting(
            TICKR_BLUETOOTH,
            "heart rate sensor",
            Role::Paired,
            &[HeartRate],
            date(2024, 5, 30),
        ),
        // The watch broadcasting its wrist reading to the Edge, under its own serial.
        sighting(
            FORERUNNER_165,
            "Garmin optical heart rate",
            Role::Paired,
            &[HeartRate],
            date(2025, 3, 2),
        ),
        sighting(
            FORERUNNER_165,
            "Forerunner 165",
            Role::Recorder,
            &[HeartRate],
            date(2024, 12, 23),
        ),
        sighting(
            FORERUNNER_165,
            "Forerunner 165",
            Role::Recorder,
            &[HeartRate],
            date(2026, 9, 29),
        ),
    ]
    .into_iter()
    .collect()
}

fn find<'a>(
    devices: &'a [domain::device::Device],
    identifier: &DeviceIdentifier,
) -> Option<&'a domain::device::Device> {
    devices.iter().find(|device| device.is(identifier))
}

/// The baseline is the first and last recording on each device, one entry per
/// serial number, and a watch is called what it calls itself.
#[test]
fn the_baseline_is_the_first_and_last_recording_on_each_device() {
    let devices = register(&record().expect("the record"), &[]);

    assert_eq!(devices.len(), 4);
    let watch = find(&devices, &serial(FORERUNNER_165)).expect("the device is in the register");
    assert_eq!(watch.name().value.as_str(), "Forerunner 165");
    assert_eq!(watch.first_used().value, date(2024, 12, 23));
    assert_eq!(watch.last_used().value, LastUsed::On(date(2026, 9, 29)));
    assert_eq!(
        watch.last_used().basis,
        Basis::Recorded { source: "Garmin" }
    );
    assert_eq!(watch.measures(), &BTreeSet::from([Measurand::HeartRate]));
}

/// The case the register exists for: rides after the watch's last Garmin
/// activity, which the record alone leaves unattributed.
#[test]
fn a_device_stated_still_in_use_measures_what_comes_after_its_last_recording() {
    let before = register(&record().expect("the record"), &[]);
    for ride in [date(2026, 9, 30), date(2026, 10, 4), date(2026, 10, 7)] {
        assert_eq!(sent_by(&before, Measurand::HeartRate, ride), InUse::Unknown);
    }

    let statements = [stated(
        date(2026, 10, 13),
        Statement::About(DeviceStatement::LastUsed {
            device: serial(FORERUNNER_165),
            last: LastUsed::StillInUse,
        }),
    )
    .expect("a statement")];
    let after = register(&record().expect("the record"), &statements);
    for ride in [date(2026, 9, 30), date(2026, 10, 4), date(2026, 10, 7)] {
        let InUse::One(device) = sent_by(&after, Measurand::HeartRate, ride) else {
            panic!("one device measured heart rate on {ride}");
        };
        assert_eq!(device.name().value.as_str(), "Forerunner 165");
    }
    let watch = find(&after, &serial(FORERUNNER_165)).expect("the device is in the register");
    assert!(matches!(watch.last_used().basis, Basis::Stated { .. }));
}

/// Where two devices that send a heart rate were in use, nothing says which
/// one did, so the ride is not attributed (§ 37).
#[test]
fn two_devices_in_use_are_several_candidates_and_not_a_guess() {
    let devices = register(&record().expect("the record"), &[]);

    // The strap, by both of its numbers; the watch beside it is no candidate
    // until it is said to send.
    let InUse::Several(candidates) = sent_by(&devices, Measurand::HeartRate, date(2024, 2, 1))
    else {
        panic!("the strap was in use under two identifiers");
    };
    assert_eq!(candidates.len(), 2);
    assert_eq!(
        sent_by(&devices, Measurand::HeartRate, date(2024, 8, 1)),
        InUse::Unknown
    );
}

/// No recording shows the Vivoactive 3 sending anything: a watch broadcasting
/// to the Bike+ leaves no trace in Garmin. Once the operator says it sends
/// heart rate, it is the candidate it was.
#[test]
fn a_device_said_to_send_is_a_candidate() {
    let statements = [stated(
        date(2026, 10, 13),
        Statement::About(DeviceStatement::Sends {
            device: serial(VIVOACTIVE_3),
            what: BTreeSet::from([Measurand::HeartRate]),
        }),
    )
    .expect("a statement")];
    let devices = register(&record().expect("the record"), &statements);

    let InUse::One(watch) = sent_by(&devices, Measurand::HeartRate, date(2024, 8, 1)) else {
        panic!("only the watch was in use");
    };
    assert_eq!(watch.name().value.as_str(), "Vivoactive 3");
    let InUse::Several(candidates) = sent_by(&devices, Measurand::HeartRate, date(2024, 2, 1))
    else {
        panic!("the watch and the strap were both in use");
    };
    assert_eq!(candidates.len(), 3);
}

/// A scale measures a pulse for itself and sends it nowhere, so it is never a
/// candidate for a ride's heart rate, however long it was in use.
#[test]
fn a_device_that_sends_nothing_is_no_candidate() {
    let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
    let scale = |on: Date| Sighting {
        device: DeviceIdentifier::Issued(IssuedId {
            platform: Platform::try_from("Withings").expect("a platform"),
            id: SerialNumber::try_from("9883ca686c39eca2f2bad69a72e2e0e6af8f71fa").expect("an id"),
        }),
        also: Vec::new(),
        name: DeviceName::try_from("Body Scan").expect("a name"),
        role: Role::Recorder,
        measured: BTreeSet::from([
            Measurand::BodyMass,
            Measurand::BodyComposition,
            Measurand::HeartRate,
        ]),
        sent: BTreeSet::new(),
        at: StartedAt::new(
            on.at(7, 0, 0, 0)
                .to_zoned(zone.as_time_zone())
                .expect("a time")
                .timestamp(),
            zone.clone(),
        ),
        source: "Withings",
    };
    let mut sightings = record().expect("the record");
    sightings.extend([scale(date(2026, 1, 11)), scale(date(2026, 10, 1))]);
    let statements = [stated(
        date(2026, 10, 13),
        Statement::About(DeviceStatement::LastUsed {
            device: serial(FORERUNNER_165),
            last: LastUsed::StillInUse,
        }),
    )
    .expect("a statement")];
    let devices = register(&sightings, &statements);

    let InUse::One(watch) = sent_by(&devices, Measurand::HeartRate, date(2026, 10, 4)) else {
        panic!("the watch alone sent the ride's heart rate");
    };
    assert_eq!(watch.name().value.as_str(), "Forerunner 165");
}

/// One strap reporting two numbers is one device once the operator says so,
/// and its period covers both.
#[test]
fn two_identifiers_stated_to_be_one_device_are_one_entry() {
    let statements = [stated(
        date(2026, 10, 13),
        Statement::About(DeviceStatement::SameDevice {
            device: serial(TICKR_ANT),
            other: serial(TICKR_BLUETOOTH),
        }),
    )
    .expect("a statement")];
    let devices = register(&record().expect("the record"), &statements);

    assert_eq!(devices.len(), 3);
    let strap = find(&devices, &serial(TICKR_BLUETOOTH)).expect("the device is in the register");
    assert!(strap.is(&serial(TICKR_ANT)));
    assert_eq!(strap.first_used().value, date(2021, 7, 17));
    assert_eq!(strap.last_used().value, LastUsed::On(date(2024, 6, 30)));
}

/// The latest statement of a kind stands, and the one before is kept rather
/// than overwritten (§ 13) — so the register is a function of all of them.
#[test]
fn the_latest_statement_stands() {
    let statements = [
        stated(
            date(2026, 10, 14),
            Statement::About(DeviceStatement::Named {
                device: serial(TICKR_ANT),
                name: DeviceName::try_from("Wahoo TICKR").expect("a name"),
            }),
        )
        .expect("a statement"),
        stated(
            date(2026, 10, 13),
            Statement::About(DeviceStatement::Named {
                device: serial(TICKR_ANT),
                name: DeviceName::try_from("TICKR").expect("a name"),
            }),
        )
        .expect("a statement"),
    ];
    let devices = register(&record().expect("the record"), &statements);

    assert_eq!(
        find(&devices, &serial(TICKR_ANT))
            .expect("the device is in the register")
            .name()
            .value
            .as_str(),
        "Wahoo TICKR"
    );
}

/// The monitor the Bike+ is paired with is named by its ANT+ number alone,
/// which is the low sixteen bits of the watch's serial: so Peloton saying it
/// is paired today is the Forerunner 165 in use today, with nobody saying so.
#[test]
fn the_monitor_the_bike_is_paired_with_is_the_watch_with_that_number() {
    let zone = OperatorZone::try_from("Europe/London".to_owned()).expect("a zone");
    let paired = Sighting {
        device: DeviceIdentifier::Ant(AntDeviceNumber::new(
            std::num::NonZeroU16::new(3629).expect("non-zero"),
        )),
        also: Vec::new(),
        name: DeviceName::try_from("Device: 3629").expect("a name"),
        role: Role::Paired,
        measured: BTreeSet::from([Measurand::HeartRate]),
        sent: BTreeSet::from([Measurand::HeartRate]),
        at: StartedAt::new(
            date(2026, 10, 13)
                .at(20, 0, 0, 0)
                .to_zoned(zone.as_time_zone())
                .expect("a time")
                .timestamp(),
            zone,
        ),
        source: "Peloton",
    };
    let mut sightings = record().expect("the record");
    sightings.push(paired);
    let devices = register(&sightings, &[]);

    assert_eq!(devices.len(), 4);
    let watch = find(&devices, &serial(FORERUNNER_165)).expect("the watch");
    assert_eq!(watch.name().value.as_str(), "Forerunner 165");
    assert_eq!(watch.last_used().value, LastUsed::On(date(2026, 10, 13)));
    assert_eq!(
        watch.last_used().basis,
        Basis::Recorded { source: "Peloton" }
    );
    let InUse::One(device) = sent_by(&devices, Measurand::HeartRate, date(2026, 10, 7)) else {
        panic!("the watch sent the ride's heart rate");
    };
    assert!(device.is(&serial(FORERUNNER_165)));
}

/// A device no recording names is added, and is told apart by the number the
/// store gave the statement.
#[test]
fn a_device_no_recording_names_can_be_added() {
    let bike = StatedDevice::new(std::num::NonZeroU64::new(1).expect("one"));
    let statements = [stated(
        date(2026, 10, 13),
        Statement::Added {
            device: bike,
            added: AddedDevice::new(
                DeviceName::try_from("Bike+").expect("a name"),
                BTreeSet::from([Measurand::Power, Measurand::Cadence, Measurand::Speed]),
                date(2023, 12, 21),
                LastUsed::StillInUse,
            )
            .expect("an ordered period"),
        },
    )
    .expect("a statement")];
    let devices = register(&record().expect("the record"), &statements);

    let added =
        find(&devices, &DeviceIdentifier::Stated(bike)).expect("the device is in the register");
    assert!(added.in_use_on(date(2026, 10, 7)));
    assert!(added.measures().contains(&Measurand::Power));
}

/// A statement that would put a device's last use before its first is refused
/// when made, and changes nothing if it is in the store anyway.
#[test]
fn a_last_use_before_the_first_is_refused() {
    let statement = stated(
        date(2026, 10, 13),
        Statement::About(DeviceStatement::LastUsed {
            device: serial(FORERUNNER_165),
            last: LastUsed::On(date(2024, 1, 1)),
        }),
    )
    .expect("a statement");
    let devices = register(&record().expect("the record"), &[]);

    assert!(matches!(
        check(&devices, &statement),
        Err(Contradiction::LastBeforeFirst { .. })
    ));
    assert_eq!(
        register(&record().expect("the record"), &[statement]),
        devices
    );
}

/// A device added with its last use before its first cannot be built.
#[test]
fn an_added_device_is_ordered_by_construction() {
    assert!(matches!(
        AddedDevice::new(
            DeviceName::try_from("Bike+").expect("a name"),
            BTreeSet::new(),
            date(2024, 1, 1),
            LastUsed::On(date(2023, 1, 1)),
        ),
        Err(Contradiction::LastBeforeFirst { .. })
    ));
}

/// A statement about a device the register does not hold is refused.
#[test]
fn a_statement_about_no_device_is_refused() {
    let statement = stated(
        date(2026, 10, 13),
        Statement::About(DeviceStatement::LastUsed {
            device: serial("42"),
            last: LastUsed::StillInUse,
        }),
    )
    .expect("a statement");

    assert_eq!(
        check(&register(&record().expect("the record"), &[]), &statement),
        Err(Contradiction::UnknownDevice(serial("42")))
    );
}

proptest! {
    /// An identifier survives being written into a page and read back out.
    #[test]
    fn an_identifier_reads_back_as_itself(serial in "[0-9A-Za-z:._-]{1,24}", number in 1u64..) {
        for identifier in [
            DeviceIdentifier::Serial(SerialNumber::try_from(serial.clone()).expect("a serial")),
            DeviceIdentifier::Issued(IssuedId {
                platform: Platform::try_from("Withings").expect("a platform"),
                id: SerialNumber::try_from(serial).expect("an id"),
            }),
            DeviceIdentifier::Stated(StatedDevice::new(std::num::NonZeroU64::new(number).expect("non-zero"))),
        ] {
            prop_assert_eq!(identifier.to_string().parse::<DeviceIdentifier>(), Ok(identifier));
        }
    }

    /// Whatever the operator states, every device's period is ordered.
    #[test]
    fn every_period_is_ordered(offsets in proptest::collection::vec((0usize..3, -4000i32..4000, any::<bool>()), 0..12)) {
        let identifiers = [serial(FORERUNNER_165), serial(VIVOACTIVE_3), serial(TICKR_ANT)];
        let origin = date(2020, 1, 1);
        let statements: Vec<Stated> = offsets
            .iter()
            .enumerate()
            .map(|(index, (which, offset, first))| {
                let device = identifiers[*which].clone();
                let on = origin.checked_add(jiff::Span::new().days(*offset)).expect("a date");
                let statement = if *first {
                    Statement::About(DeviceStatement::FirstUsed { device, on })
                } else {
                    Statement::About(DeviceStatement::LastUsed { device, last: LastUsed::On(on) })
                };
                Stated {
                    at: Timestamp::from_second(i64::try_from(index).expect("small")).expect("an instant"),
                    statement,
                }
            })
            .collect();
        for device in register(&record().expect("the record"), &statements) {
            prop_assert!(device.last_used().value.covers(device.first_used().value));
        }
    }
}
