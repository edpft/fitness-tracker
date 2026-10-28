//! Garmin's sleep record: a night's sleep (#255), and which watch it names
//! (#569).
//!
//! **Garmin's HRV and daily heart rate name no device; its sleep record does**,
//! for the same date. `dailySleepDTO.sleepNeed.deviceId` is the one field there
//! on every night since 2024-12-22, and on every night it agrees with the `SpO2`
//! summary's and the daily summary's where they state one. It is the watch's
//! unit id, which is its serial number.
//!
//! **Translating a night is deterministic and total**, as the other
//! translators are (§ 9). One landing record is one night, so there is nothing
//! to compose. A night Garmin recorded no sleep on is refused as that
//! ([`RefusalReason::NoSleepRecorded`]), not dropped (§ 37).
//!
//! **The window is read from Garmin's GMT instants** and placed in the zone in
//! force then (§ 13), as HRV's is. The `Local` stamps beside them are the
//! watch's offset at the time and are not carried (§ II.3).

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    body::{Sleep, SleepRecord, SleepWindow},
    device::{DeviceIdentifier, SerialNumber},
    landing::{EventKind, LandedRecord},
    measure::PositiveDuration,
    normalised::{RefusalLocus, RefusalReason, StatedZones},
};
use jiff::civil::Date;
use serde::Deserialize;

use crate::scribe::Scribe;

use super::account::SleepAccount;

#[derive(Debug, Deserialize)]
struct Served {
    #[serde(rename = "dailySleepDTO")]
    daily: Option<Daily>,
}

#[derive(Debug, Deserialize)]
struct Daily {
    #[serde(rename = "sleepNeed")]
    need: Option<Need>,
    #[serde(rename = "calendarDate")]
    date: Option<String>,
    #[serde(rename = "sleepTimeSeconds")]
    asleep: Option<u64>,
    #[serde(rename = "sleepStartTimestampGMT")]
    fell_asleep: Option<i64>,
    #[serde(rename = "sleepEndTimestampGMT")]
    woke: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct Need {
    #[serde(rename = "deviceId")]
    device: Option<u64>,
}

/// The watch the record names, or [`None`] where it names none or will not
/// read: no evidence about any device.
#[must_use]
pub fn recorded_by(sleep: &LandedRecord) -> Option<DeviceIdentifier> {
    let served: Served = serde_json::from_slice(sleep.payload().as_bytes()).ok()?;
    let device = served.daily?.need?.device?;
    SerialNumber::try_from(device.to_string())
        .ok()
        .map(DeviceIdentifier::Serial)
}

/// A millisecond instant, as Garmin serves the window's ends.
fn instant(milliseconds: i64, field: &'static str) -> Result<jiff::Timestamp, RefusalReason> {
    jiff::Timestamp::from_millisecond(milliseconds).map_err(|error| {
        RefusalReason::UnreadableValue {
            field,
            detail: format!("{milliseconds} is not an instant: {error}"),
        }
    })
}

/// The night's sleep, or the one reason there is none.
fn sleep(record: &LandedRecord, zones: &StatedZones) -> Result<Sleep, RefusalReason> {
    let served: Served = serde_json::from_slice(record.payload().as_bytes()).map_err(|error| {
        RefusalReason::UnreadablePayload {
            detail: error.to_string(),
        }
    })?;
    let daily = served
        .daily
        .ok_or_else(|| RefusalReason::UnreadablePayload {
            detail: "a night with no dailySleepDTO".to_owned(),
        })?;
    let asleep = daily.asleep.ok_or(RefusalReason::NoSleepRecorded)?;
    let asleep =
        PositiveDuration::from_seconds(asleep).map_err(|error| RefusalReason::UnreadableValue {
            field: "time asleep",
            detail: error.to_string(),
        })?;
    let (Some(fell_asleep), Some(woke)) = (daily.fell_asleep, daily.woke) else {
        return Err(RefusalReason::UnreadablePayload {
            detail: "a night with time asleep and no window".to_owned(),
        });
    };
    let window = SleepWindow::new(
        zones.started_at(instant(fell_asleep, "fell asleep")?),
        zones.started_at(instant(woke, "woke")?),
    )
    .map_err(|error| RefusalReason::UnreadableValue {
        field: "sleep window",
        detail: error.to_string(),
    })?;
    let stated = daily.date.ok_or_else(|| RefusalReason::UnreadablePayload {
        detail: "a night with no calendarDate".to_owned(),
    })?;
    let morning_of: Date =
        stated
            .parse()
            .map_err(|error: jiff::Error| RefusalReason::UnreadableValue {
                field: "calendar date",
                detail: format!("{stated:?} is not a date: {error}"),
            })?;
    Sleep::new(SleepRecord {
        morning_of,
        window,
        asleep,
        landed_as: record.id(),
        source_record_id: record.source_record_id().clone(),
        provenance: record.provenance().clone(),
        recorded_by: recorded_by(record),
    })
    .map_err(|error| RefusalReason::UnreadableValue {
        field: "time asleep",
        detail: error.to_string(),
    })
}

/// The Garmin adapter's translator for a night's sleep.
#[derive(Debug, Clone, Copy, Default)]
pub struct GarminSleepTranslator;

impl Translator for GarminSleepTranslator {
    type Account = SleepAccount;
    type Entity = Sleep;

    fn translate(
        &self,
        account: &SleepAccount,
        zones: &StatedZones,
    ) -> Result<Translation<Sleep>, NormalisationError> {
        let record = account.sleep();
        let mut scribe = Scribe::new(record);

        let event = crate::store::served_by_a_feed(record.provenance())?;
        match event.kind() {
            EventKind::Deleted => {
                return Ok(Translation::Retraction {
                    of: record.source_record_id().clone(),
                });
            }
            EventKind::Unrecognised(kind) => {
                return Ok(scribe.only(
                    RefusalLocus::Record,
                    RefusalReason::UnreadablePayload {
                        detail: format!("event kind {kind:?} is not one we translate"),
                    },
                ));
            }
            EventKind::Updated => {}
        }

        Ok(match sleep(record, zones) {
            Ok(night) => Translation::Entity {
                entity: Box::new(night),
                refusals: scribe.into_refusals(),
            },
            Err(reason) => scribe.only(RefusalLocus::Record, reason),
        })
    }
}
