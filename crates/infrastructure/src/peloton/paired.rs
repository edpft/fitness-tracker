//! The heart rate monitors paired to the Bike+, from Peloton's account (#427).
//!
//! **The only thing Peloton says about where a ride's heart rate came from.** A
//! ride names no device, but `/api/me` lists what the bike is paired with as it
//! stands when asked: each monitor by the number it broadcasts by over ANT+,
//! which is the low sixteen bits of its serial — so the monitor a watch's
//! recordings already name. Landed whenever the answer changes, the list is a
//! history from the first extract on.

use std::collections::BTreeSet;

use application::{EventBatch, SourceError, WorkoutEventSource};
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

/// A heart rate monitor paired to the Bike+, as Peloton names it.
///
/// **All Peloton says of one**: the number it broadcasts by and a name, and
/// that the bike takes its heart rate.
#[must_use]
pub fn monitor(device: DeviceIdentifier, name: DeviceName, at: StartedAt) -> Sighting {
    Sighting {
        device,
        also: Vec::new(),
        name,
        role: Role::Paired,
        worn: false,
        measured: BTreeSet::from([Measurand::HeartRate]),
        sent: BTreeSet::from([Measurand::HeartRate]),
        at,
        source: SOURCE,
    }
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
            Some(monitor(device, name, at.clone()))
        })
        .collect()
}

/// The monitor a ride starting `at` was ridden with: the one both answers
/// either side of it name (operator, 2026-10-25).
///
/// **One answer at or before the ride and one at or after it, each naming one
/// monitor, and the same one.** A ride before the first answer or after the
/// last has only one side; two answers naming different monitors, or either
/// naming two, leave which one it was unsaid. `pairings` is oldest first.
#[must_use]
pub fn ridden_with(pairings: &[LandedRecord], at: &StartedAt) -> Option<Sighting> {
    let (before, after) = either_side(pairings, at.instant());
    let (before, after) = (before?, after?);
    let (named, also) = (sightings(before, at.zone()), sightings(after, at.zone()));
    let ([named], [also]) = (named.as_slice(), also.as_slice()) else {
        return None;
    };
    (named.device == also.device).then(|| Sighting {
        at: at.clone(),
        ..named.clone()
    })
}

/// The answers either side of an instant: the last at or before it and the
/// first at or after it, which are all [`ridden_with`] reads of the account.
#[must_use]
pub fn either_side(
    pairings: &[LandedRecord],
    instant: jiff::Timestamp,
) -> (Option<&LandedRecord>, Option<&LandedRecord>) {
    let when = |landed: &LandedRecord| landed.record().fetched_at().as_timestamp();
    let before = pairings
        .iter()
        .filter(|landed| when(landed) <= instant)
        .max_by_key(|landed| (when(landed), landed.id()));
    let after = pairings
        .iter()
        .filter(|landed| when(landed) >= instant)
        .min_by_key(|landed| (when(landed), landed.id()));
    (before, after)
}

#[cfg(test)]
mod tests {
    use domain::{
        device::DeviceIdentifier,
        landing::{
            Endpoint, EventKind, EventProvenance, FetchedAt, LandedRecord, LandingRecord,
            LandingRecordId, LandingStream, RawPayload, SourceRecordId,
        },
        normalised::{OperatorZone, StartedAt},
    };
    use serde_json::json;

    use super::ridden_with;

    type Failure = Box<dyn std::error::Error>;

    /// The account as fetched at `at`, paired with `monitors`.
    fn answer(id: i64, at: &str, monitors: &[&str]) -> Result<LandedRecord, Failure> {
        let paired: Vec<_> = monitors
            .iter()
            .map(|number| {
                json!({
                    "name": format!("Device: {number}"),
                    "paired_device_type": "heart_rate_monitor",
                    "serial_number": number,
                })
            })
            .collect();
        let payload = serde_json::to_vec(&json!({ "id": "me", "paired_devices": paired }))?;
        Ok(LandedRecord::new(
            LandingRecordId::try_from(id)?,
            LandingRecord::land(
                LandingStream::try_from("peloton.account")?,
                FetchedAt::try_from(at)?,
                SourceRecordId::try_from("me")?,
                EventProvenance::new(Endpoint::try_from("/api/me")?, EventKind::Updated, None)
                    .into(),
                RawPayload::try_from(payload)?,
            ),
        ))
    }

    fn ride(at: &str) -> Result<StartedAt, Failure> {
        Ok(StartedAt::new(
            at.parse()?,
            OperatorZone::try_from("Europe/London")?,
        ))
    }

    /// What a monitor named on a ride starting `at` broadcasts as.
    fn number(pairings: &[LandedRecord], at: &str) -> Option<String> {
        let seen = ridden_with(pairings, &ride(at).ok()?)?;
        Some(match seen.device {
            DeviceIdentifier::Ant(number) => number.get().to_string(),
            DeviceIdentifier::Serial(serial) => serial.to_string(),
        })
    }

    /// The operator's rule, 2026-10-25, on his dates: the first answer was
    /// 2026-10-14T13:00Z, and he rode at 20:03 that evening.
    #[test]
    fn a_ride_between_two_answers_naming_one_monitor_was_ridden_with_it() {
        let pairings = [
            answer(1, "2026-10-14T13:00:31Z", &["3629"]).expect("an answer"),
            answer(2, "2026-10-15T06:00:00Z", &["3629"]).expect("an answer"),
            answer(3, "2026-10-16T06:00:00Z", &["10341"]).expect("an answer"),
        ];

        assert_eq!(
            number(&pairings, "2026-10-14T20:03:30Z"),
            Some("3629".to_owned())
        );
        let between = ridden_with(&pairings, &ride("2026-10-14T20:03:30Z").expect("a ride"))
            .expect("a monitor");
        assert_eq!(between.at, ride("2026-10-14T20:03:30Z").expect("a ride"));

        assert_eq!(
            number(&pairings, "2026-10-07T20:27:29Z"),
            None,
            "before the first answer"
        );
        assert_eq!(
            number(&pairings, "2026-10-15T19:00:00Z"),
            None,
            "between answers that differ"
        );
        assert_eq!(
            number(&pairings, "2026-10-17T19:00:00Z"),
            None,
            "after the last answer"
        );
    }

    #[test]
    fn two_monitors_paired_name_neither() {
        let pairings = [
            answer(1, "2026-10-14T13:00:00Z", &["3629", "10341"]).expect("an answer"),
            answer(2, "2026-10-15T13:00:00Z", &["3629", "10341"]).expect("an answer"),
        ];
        assert_eq!(number(&pairings, "2026-10-14T20:00:00Z"), None);
    }
}
