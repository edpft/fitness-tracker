//! Turning one day of Garmin's all-day heart rate into a [`RestingHeartRate`].
//!
//! Deterministic and total, as the other translators are (§ 9). One landing
//! record is one day, so there is nothing to compose: § II.3.1's degenerate
//! case, as a night of HRV is.
//!
//! **A day nothing was worn is refused as that** ([`RefusalReason::NothingWorn`]),
//! not dropped (§ 37): Garmin serves it with no resting heart rate, and that is
//! the record saying the day is a gap.

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    body::{RestingHeartRate, RestingHeartRateRecord},
    landing::{EventKind, LandedRecord},
    measure::BeatsPerMinute,
    normalised::{OperatorZone, RefusalLocus, RefusalReason},
};
use jiff::civil::Date;
use serde::Deserialize;

use crate::scribe::Scribe;

use super::account::DayAccount;

#[derive(Debug, Deserialize)]
struct Day {
    #[serde(rename = "calendarDate")]
    date: Option<String>,
    #[serde(rename = "restingHeartRate")]
    resting: Option<u32>,
}

/// The day's figure, or the one reason there is none.
fn resting_heart_rate(record: &LandedRecord) -> Result<RestingHeartRate, RefusalReason> {
    let day: Day = serde_json::from_slice(record.payload().as_bytes()).map_err(|error| {
        RefusalReason::UnreadablePayload {
            detail: error.to_string(),
        }
    })?;
    let rate = day.resting.ok_or(RefusalReason::NothingWorn)?;
    let rate = BeatsPerMinute::new(rate).map_err(|error| RefusalReason::UnreadableValue {
        field: "resting heart rate",
        detail: error.to_string(),
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
    Ok(RestingHeartRate::new(RestingHeartRateRecord {
        day: date,
        rate,
        landed_as: record.id(),
        source_record_id: record.source_record_id().clone(),
        provenance: record.provenance().clone(),
    }))
}

/// The Garmin adapter's translator for the wrist's resting heart rate.
#[derive(Debug, Clone, Copy, Default)]
pub struct GarminRestingHeartRateTranslator;

impl Translator for GarminRestingHeartRateTranslator {
    type Account = DayAccount;
    type Entity = RestingHeartRate;

    fn translate(
        &self,
        account: &DayAccount,
        _zone: &OperatorZone,
    ) -> Result<Translation<RestingHeartRate>, NormalisationError> {
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

        Ok(match resting_heart_rate(record) {
            Ok(day) => Translation::Entity {
                entity: Box::new(day),
                refusals: scribe.into_refusals(),
            },
            Err(reason) => scribe.only(RefusalLocus::Record, reason),
        })
    }
}
