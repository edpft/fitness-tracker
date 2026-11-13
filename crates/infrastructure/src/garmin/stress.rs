//! Turning one day of Garmin's daily summary into a [`Stress`] (#517, #777).
//!
//! Deterministic and total, as the other translators are (§ 9). One landing
//! record is one day, so there is nothing to compose: § II.3.1's degenerate
//! case, as a day's resting heart rate is.
//!
//! **The daily summary, not the stress stream**: both state the same average
//! on every one of the operator's days, and only the summary says how much of
//! the day it was measured over.
//!
//! **A day with no average is refused as that**
//! ([`RefusalReason::NoStressRecorded`]), not dropped (§ 37). Garmin serves
//! such a day with no figure, or with −1 for one.

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    body::{Stress, StressLevel, StressRecord},
    device::DeviceIdentifier,
    landing::{EventKind, LandedRecord},
    measure::PositiveDuration,
    normalised::{RefusalLocus, RefusalReason, StatedZones},
};
use jiff::civil::Date;
use serde::Deserialize;

use crate::scribe::Scribe;

use super::account::DayAccount;

#[derive(Debug, Deserialize)]
struct Day {
    #[serde(rename = "calendarDate")]
    date: Option<String>,
    #[serde(rename = "averageStressLevel")]
    average: Option<i64>,
    #[serde(rename = "totalStressDuration")]
    measured: Option<u64>,
}

/// The day's stress, or the one reason there is none.
fn stress(
    record: &LandedRecord,
    recorded_by: Option<DeviceIdentifier>,
) -> Result<Stress, RefusalReason> {
    let day: Day = serde_json::from_slice(record.payload().as_bytes()).map_err(|error| {
        RefusalReason::UnreadablePayload {
            detail: error.to_string(),
        }
    })?;
    let average = day
        .average
        .filter(|level| *level > 0)
        .ok_or(RefusalReason::NoStressRecorded)?;
    let average = StressLevel::new(average).map_err(|error| RefusalReason::UnreadableValue {
        field: "average stress level",
        detail: error.to_string(),
    })?;
    let measured = day
        .measured
        .ok_or_else(|| RefusalReason::UnreadablePayload {
            detail: "a day with an average stress level and no totalStressDuration".to_owned(),
        })?;
    let measured = PositiveDuration::from_seconds(measured).map_err(|error| {
        RefusalReason::UnreadableValue {
            field: "total stress duration",
            detail: error.to_string(),
        }
    })?;
    let stated = day.date.ok_or_else(|| RefusalReason::UnreadablePayload {
        detail: "a day with no calendarDate".to_owned(),
    })?;
    let date: Date =
        stated
            .parse()
            .map_err(|error: jiff::Error| RefusalReason::UnreadableValue {
                field: "calendar date",
                detail: format!("{stated:?} is not a date: {error}"),
            })?;
    Ok(Stress::new(StressRecord {
        day: date,
        average,
        measured,
        landed_as: record.id(),
        source_record_id: record.source_record_id().clone(),
        provenance: record.provenance().clone(),
        recorded_by,
    }))
}

/// The Garmin adapter's translator for a day's stress.
#[derive(Debug, Clone, Copy, Default)]
pub struct GarminStressTranslator;

impl Translator for GarminStressTranslator {
    type Account = DayAccount;
    type Entity = Stress;
    const VERSION: u32 = 1;

    fn translate(
        &self,
        account: &DayAccount,
        _zones: &StatedZones,
    ) -> Result<Translation<Stress>, NormalisationError> {
        let record = account.day();
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

        Ok(
            match stress(record, account.sleep().and_then(super::sleep::recorded_by)) {
                Ok(day) => Translation::Entity {
                    entity: Box::new(day),
                    refusals: scribe.into_refusals(),
                },
                Err(reason) => scribe.only(RefusalLocus::Record, reason),
            },
        )
    }
}

/// Nothing beyond the accounts it is handed (#809).
impl application::graph::Input for GarminStressTranslator {
    fn reads() -> application::graph::Reads {
        application::graph::Reads::default()
    }
}
