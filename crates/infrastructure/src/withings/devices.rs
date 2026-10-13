//! What Withings' measurements say about the scale that took them (#427).
//!
//! **Every group names its device**, by an id Withings issues (`deviceid`), and
//! its model. A group naming none was typed in by hand rather than measured,
//! and is no evidence about any device.
//!
//! **A scale sends nothing.** It measures a pulse for itself, so it is never a
//! candidate for a heart rate another device relays (operator, 2026-10-13).

use std::collections::BTreeSet;

use domain::{
    device::{DeviceIdentifier, DeviceName, IssuedId, Measurand, Platform, Role, Sighting},
    normalised::{OperatorZone, StartedAt},
};
use jiff::Timestamp;
use serde::Deserialize;

use super::{
    WeighInAccount,
    translate::{COMPOSITION, kind},
};

/// Whose measurements these are, as the register says it.
pub const SOURCE: &str = "Withings";

#[derive(Debug, Deserialize)]
struct Group {
    date: i64,
    #[serde(default)]
    timezone: Option<String>,
    #[serde(default)]
    deviceid: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    measures: Vec<Measure>,
}

#[derive(Debug, Deserialize)]
struct Measure {
    #[serde(rename = "type")]
    kind: i64,
}

/// The scale each of a weigh-in's groups names, once per device.
///
/// **Empty, never an error**, for a group that will not read or names no
/// device, for [`crate::garmin::devices::sightings`]'s reason.
#[must_use]
pub fn sightings(account: &WeighInAccount, zone: &OperatorZone) -> Vec<Sighting> {
    let mut found: Vec<Sighting> = Vec::new();
    for landed in account.groups() {
        let Ok(group) = serde_json::from_slice::<Group>(landed.payload().as_bytes()) else {
            continue;
        };
        let (Some(id), Ok(instant)) = (
            group.deviceid.as_deref(),
            Timestamp::from_second(group.date),
        ) else {
            continue;
        };
        let (Ok(platform), Ok(id)) = (Platform::try_from(SOURCE), id.parse()) else {
            continue;
        };
        let device = DeviceIdentifier::Issued(IssuedId { platform, id });
        let measured = measures(&group.measures);
        if let Some(seen) = found.iter_mut().find(|seen| seen.device == device) {
            seen.measured.extend(measured);
            continue;
        }
        let Ok(name) =
            DeviceName::try_from(group.model.unwrap_or_else(|| "Withings scale".to_owned()))
        else {
            continue;
        };
        // The zone the source states, where it states one (§ II.3).
        let zone = group
            .timezone
            .and_then(|stated| OperatorZone::try_from(stated).ok())
            .unwrap_or_else(|| zone.clone());
        found.push(Sighting {
            device,
            also: Vec::new(),
            name,
            role: Role::Recorder,
            measured,
            sent: BTreeSet::new(),
            at: StartedAt::new(instant, zone),
            source: SOURCE,
        });
    }
    found
}

/// What a group measured, of what the register names.
fn measures(measures: &[Measure]) -> BTreeSet<Measurand> {
    measures
        .iter()
        .filter_map(|measure| match measure.kind {
            kind::MASS => Some(Measurand::BodyMass),
            kind::HEART_RATE => Some(Measurand::HeartRate),
            other if COMPOSITION.contains(&other) => Some(Measurand::BodyComposition),
            _ => None,
        })
        .collect()
}
