//! The heart rate monitors paired to the Bike+, from Peloton's account (#427).
//!
//! **The only thing Peloton says about where a ride's heart rate came from.** A
//! ride names no device, but `/api/me` lists what the bike is paired with as it
//! stands when asked: each monitor by the number it broadcasts by over ANT+,
//! which is the low sixteen bits of its serial — so the monitor a watch's
//! recordings already name. Landed whenever the answer changes, the list is a
//! history from the first extract on.

use std::collections::BTreeSet;

use application::{EventBatch, SourceError, SourceEvent, WorkoutEventSource};
use domain::{
    device::{AntDeviceNumber, DeviceIdentifier, DeviceName, Measurand, Role, Sighting},
    landing::{
        Endpoint, EventKind, EventProvenance, LandedRecord, RawPayload, SourceRecordId, Watermark,
    },
    normalised::{OperatorZone, StartedAt},
};
use serde::Deserialize;

use super::{
    auth::PelotonAuth,
    workouts::{ME_ENDPOINT, PelotonWorkouts},
};

/// Whose account this is, as the register says it.
pub const SOURCE: &str = "Peloton";

/// What Peloton calls a heart rate monitor among the things a bike is paired
/// with; a speaker is `audio`.
const HEART_RATE_MONITOR: &str = "heart_rate_monitor";

/// The account, asked for once per run.
pub struct PelotonAccount {
    client: PelotonWorkouts,
}

impl PelotonAccount {
    pub fn new(api_base: impl Into<String>, auth: PelotonAuth) -> Self {
        Self {
            client: PelotonWorkouts::new(api_base, auth),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Identity {
    id: String,
}

impl WorkoutEventSource for PelotonAccount {
    /// One request is the whole of it.
    type Resume = ();

    async fn fetch(
        &self,
        _since: Option<Watermark>,
        _resume: Option<()>,
    ) -> Result<EventBatch<()>, SourceError> {
        let malformed = |detail: String| SourceError::Malformed { detail };
        let body = self.client.get(ME_ENDPOINT, &[]).await?;
        let identity: Identity = serde_json::from_slice(&body)
            .map_err(|error| malformed(format!("{ME_ENDPOINT} answered without an id: {error}")))?;
        let endpoint =
            Endpoint::try_from(ME_ENDPOINT).map_err(|error| malformed(error.to_string()))?;
        let event = crate::revision::json_event(
            SourceRecordId::try_from(identity.id).map_err(|error| malformed(error.to_string()))?,
            // The account as it stands: no event, and no time it happened.
            EventProvenance::new(endpoint, EventKind::Updated, None).into(),
            RawPayload::try_from(body).map_err(|error| malformed(error.to_string()))?,
        );
        Ok(EventBatch {
            events: vec![event],
            resume: None,
        })
    }
}

#[derive(Debug, Deserialize)]
struct Account {
    #[serde(default)]
    paired_devices: Vec<Pairing>,
}

#[derive(Debug, Deserialize)]
struct Pairing {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "paired_device_type")]
    kind: Option<String>,
    #[serde(default)]
    serial_number: Option<String>,
}

/// The monitors one landed account names, each in use when it was fetched.
///
/// **Empty, never an error**, for a record that will not read: it is no
/// evidence about any device.
#[must_use]
pub fn sightings(landed: &LandedRecord, zone: &OperatorZone) -> Vec<Sighting> {
    let Ok(account) = serde_json::from_slice::<Account>(landed.payload().as_bytes()) else {
        return Vec::new();
    };
    let at = StartedAt::new(landed.record().fetched_at().as_timestamp(), zone.clone());
    account
        .paired_devices
        .into_iter()
        .filter(|paired| paired.kind.as_deref() == Some(HEART_RATE_MONITOR))
        .filter_map(|paired| {
            let number = paired.serial_number?.parse::<u32>().ok()?;
            let device = DeviceIdentifier::Ant(AntDeviceNumber::of_serial(number)?);
            let name = paired
                .name
                .and_then(|name| DeviceName::try_from(name).ok())
                .or_else(|| DeviceName::try_from(format!("Heart rate monitor {number}")).ok())?;
            Some(Sighting {
                device,
                also: Vec::new(),
                name,
                role: Role::Paired,
                worn: false,
                measured: BTreeSet::from([Measurand::HeartRate]),
                sent: BTreeSet::from([Measurand::HeartRate]),
                at: at.clone(),
                source: SOURCE,
            })
        })
        .collect()
}
