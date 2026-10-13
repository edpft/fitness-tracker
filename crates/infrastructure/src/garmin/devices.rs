//! What Garmin's recordings say about the hardware that made them (#427).
//!
//! **Each recording names its device and every sensor paired to it**, by the
//! serial number the hardware reports, and this turns that into the register's
//! terms. FIT's codes for makers, products and kinds of sensor are Garmin's
//! profile, and stay here (§ II.3).
//!
//! **What a device measured in a recording is what it supplied to it.** A watch
//! with a strap paired supplied no heart rate, though its wrist sensor could
//! have; one with nothing paired did. So a watch worn beside a strap is not a
//! second candidate on those days.
//!
//! **A serial of 0 or 1 identifies nothing.** FIT writes 0 for "not known", and
//! both syncs that file other platforms' rides into Garmin write 1: Peloton's
//! uploads present as a Forerunner 945 and Tacx's app as itself, and neither is
//! hardware the operator used. A sensor a recording names with no serial at all
//! — 117 recordings from 2018 to 2023 name a speed and cadence sensor so — is
//! no device here, and its readings are of no known device (§ 37).

use std::collections::BTreeSet;

use domain::{
    device::{
        AntDeviceNumber, DeviceIdentifier, DeviceName, Measurand, Role, SerialNumber, Sighting,
    },
    normalised::OperatorZone,
};
use serde::Deserialize;

use super::{
    ActivityAccount,
    fit::{self, Identity, Listed},
    gym::started,
};

/// Whose recordings these are, as the register says it.
pub const SOURCE: &str = "Garmin";

/// What the activity list says that the file does not: when it started.
#[derive(Debug, Deserialize)]
struct Activity {
    #[serde(rename = "startTimeGMT")]
    started_at: Option<String>,
}

/// The devices one activity's recording names.
///
/// **Empty, never an error**, where the activity states no start or its
/// recording did not land or will not read: a recording that says nothing about
/// its hardware is no evidence about any device, and the register is a floor,
/// not a census.
#[must_use]
pub fn sightings(account: &ActivityAccount, zone: &OperatorZone) -> Vec<Sighting> {
    let Some(recording) = account.recording() else {
        return Vec::new();
    };
    let Ok(activity) = serde_json::from_slice::<Activity>(account.activity().payload().as_bytes())
    else {
        return Vec::new();
    };
    let Some(at) = activity
        .started_at
        .as_deref()
        .and_then(|stamp| started(stamp, zone).ok())
    else {
        return Vec::new();
    };
    let Ok(hardware) = fit::hardware(recording.payload().as_bytes()) else {
        return Vec::new();
    };

    let Some(recorder) = identified(hardware.recorder) else {
        return Vec::new();
    };
    if hardware.recorder.product == Some(CONNECT) {
        return Vec::new();
    }

    let mut found: Vec<Sighting> = Vec::new();
    let mut supplied_heart_rate = false;
    let mut wrist = false;
    for listed in &hardware.listed {
        if listed.link == Some(ONBOARD) {
            wrist |= listed.kind == Some(WRIST_HEART_RATE);
            continue;
        }
        let Some(serial) = identified(listed.identity) else {
            supplied_heart_rate |= measures(listed).contains(&Measurand::HeartRate);
            continue;
        };
        if serial == recorder || listed.index == Some(0) {
            continue;
        }
        let measured = measures(listed);
        if measured.is_empty() {
            continue;
        }
        supplied_heart_rate |= measured.contains(&Measurand::HeartRate);
        let Ok(device) = SerialNumber::try_from(serial.to_string()).map(DeviceIdentifier::Serial)
        else {
            continue;
        };
        if let Some(seen) = found.iter_mut().find(|seen| seen.device == device) {
            seen.sent.extend(measured.iter().copied());
            seen.measured.extend(measured);
            continue;
        }
        let Ok(name) = DeviceName::try_from(paired_name(listed.identity, &measured)) else {
            continue;
        };
        found.push(Sighting {
            device,
            // A Bluetooth id is not a serial, so its low bits are no ANT+
            // number, and a match on them would join two devices.
            also: if bluetooth(listed) {
                Vec::new()
            } else {
                broadcast(serial)
            },
            name,
            role: Role::Paired,
            // Paired is sent: whatever it measured reached the recorder.
            sent: measured.clone(),
            measured,
            at: at.clone(),
            source: SOURCE,
        });
    }

    let (Ok(device), Ok(name)) = (
        SerialNumber::try_from(recorder.to_string()).map(DeviceIdentifier::Serial),
        DeviceName::try_from(recorder_name(hardware.recorder)),
    ) else {
        return found;
    };
    let mut measured = BTreeSet::new();
    if wrist && !supplied_heart_rate {
        measured.insert(Measurand::HeartRate);
    }
    found.insert(
        0,
        Sighting {
            device,
            also: broadcast(recorder),
            name,
            role: Role::Recorder,
            measured,
            sent: BTreeSet::new(),
            at,
            source: SOURCE,
        },
    );
    found
}

/// The ANT+ number a device with this serial broadcasts by, which is how the
/// Bike+'s paired monitor names it.
fn broadcast(serial: u32) -> Vec<DeviceIdentifier> {
    AntDeviceNumber::of_serial(serial)
        .map(DeviceIdentifier::Ant)
        .into_iter()
        .collect()
}

/// The serial number, where it identifies something.
fn identified(identity: Identity) -> Option<u32> {
    identity.serial.filter(|serial| *serial > 1)
}

/// FIT's `source_type` for a sensor built into the recorder.
const ONBOARD: u8 = 5;
/// FIT's `local_device_type` for a wrist heart rate sensor.
const WRIST_HEART_RATE: u8 = 10;
/// FIT's `source_type`s for Bluetooth; every other link reads ANT+'s table.
const BLUETOOTH: u8 = 2;
const BLUETOOTH_LOW_ENERGY: u8 = 3;
/// Garmin's product code for its website, which writes manual entries.
const CONNECT: u16 = 65534;

/// Whether a sensor was connected over Bluetooth.
const fn bluetooth(listed: &Listed) -> bool {
    matches!(listed.link, Some(BLUETOOTH | BLUETOOTH_LOW_ENERGY))
}

/// What a paired sensor measures, by its kind.
///
/// ANT+ and Bluetooth number kinds differently, and a file old enough to state
/// no link is ANT+: the Forerunner 310XT had nothing else.
fn measures(listed: &Listed) -> BTreeSet<Measurand> {
    use Measurand::{Cadence, HeartRate, Power, Speed};
    let measured: &[Measurand] = match (bluetooth(listed), listed.kind) {
        (false, Some(120)) | (true, Some(1)) => &[HeartRate],
        (false, Some(121)) | (true, Some(3)) => &[Speed, Cadence],
        (false, Some(122)) | (true, Some(5)) => &[Cadence],
        (false, Some(123)) | (true, Some(4)) => &[Speed],
        (false, Some(11)) | (true, Some(2)) => &[Power],
        _ => &[],
    };
    measured.iter().copied().collect()
}

/// Who made it, where the file says and the profile names them.
const fn maker(identity: Identity) -> Option<&'static str> {
    match identity.manufacturer {
        Some(1) => Some("Garmin"),
        Some(32) => Some("Wahoo"),
        Some(89) => Some("Tacx"),
        Some(340) => Some("Peloton"),
        _ => None,
    }
}

/// What a recorder is called: its model, where the profile names one the
/// operator used, or its maker's product number.
///
/// Checked against Garmin's FIT SDK profile (`garminProduct`), 2026-10-13.
fn recorder_name(identity: Identity) -> String {
    let model = match (identity.manufacturer, identity.product) {
        (Some(1), Some(1018)) => Some("Forerunner 310XT"),
        (Some(1), Some(1482)) => Some("Forerunner 10"),
        (Some(1), Some(2700)) => Some("Vivoactive 3"),
        (Some(1), Some(3121)) => Some("Edge 530"),
        (Some(1), Some(4432)) => Some("Forerunner 165"),
        _ => None,
    };
    match (model, maker(identity), identity.product) {
        (Some(model), _, _) => model.to_owned(),
        (None, Some(maker), Some(product)) => format!("{maker} product {product}"),
        (None, Some(maker), None) => format!("{maker} device"),
        (None, None, _) => "Recording device".to_owned(),
    }
}

/// What a paired sensor is called: its maker, where stated, and what it
/// measures.
fn paired_name(identity: Identity, measured: &BTreeSet<Measurand>) -> String {
    let kind = if measured.contains(&Measurand::HeartRate) {
        "heart rate sensor"
    } else if measured.contains(&Measurand::Power) {
        "power meter"
    } else if measured.contains(&Measurand::Speed) && measured.contains(&Measurand::Cadence) {
        "speed and cadence sensor"
    } else if measured.contains(&Measurand::Speed) {
        "speed sensor"
    } else {
        "cadence sensor"
    };
    maker(identity).map_or_else(
        || {
            let mut named = kind.to_owned();
            if let Some(first) = named.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            named
        },
        |maker| format!("{maker} {kind}"),
    )
}
