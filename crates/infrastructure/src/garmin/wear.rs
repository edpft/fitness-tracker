//! What one day of Garmin's all-day heart rate derives: its resting heart rate,
//! and whether a watch was worn (#569).
//!
//! **A sum for [`super::activity`]'s reason**: one landed day says both.
//!
//! **Worn if the day states a resting heart rate or any reading at all.** A day
//! the account holds and shows neither is a day nothing was worn, which is a
//! fact about the day rather than a gap in it — so it derives a day not worn
//! as well as a refusal of its resting heart rate.

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    body::RestingHeartRate,
    landing::{LandedRecord, LandingRecordId, SourceRecordId},
    normalised::{NormalisedEntity, StatedZones},
    sequence::NonEmpty,
};
use jiff::civil::Date;
use serde::Deserialize;

use super::{DayAccount, GarminRestingHeartRateTranslator};

/// Whether a wrist device was worn on one day, as the account's daily heart
/// rate says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WearDay {
    pub day: Date,
    pub worn: bool,
    pub landed_as: LandingRecordId,
    pub source_record_id: SourceRecordId,
}

impl NormalisedEntity for WearDay {
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.source_record_id]
    }
}

/// Either of what one day derives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GarminDayEntity {
    RestingHeartRate(RestingHeartRate),
    Wear(WearDay),
}

impl NormalisedEntity for GarminDayEntity {
    fn composes(&self) -> Vec<&SourceRecordId> {
        match self {
            Self::RestingHeartRate(day) => day.composes(),
            Self::Wear(day) => day.composes(),
        }
    }
}

/// What a day's answer says about wear. Everything else stays in the payload.
#[derive(Debug, Deserialize)]
struct Day {
    #[serde(default, rename = "restingHeartRate")]
    resting: Option<serde_json::Value>,
    #[serde(default, rename = "heartRateValues")]
    readings: Option<Vec<(Option<i64>, Option<i64>)>>,
}

impl Day {
    fn worn(&self) -> bool {
        self.resting
            .as_ref()
            .is_some_and(|resting| !resting.is_null())
            || self
                .readings
                .as_ref()
                .is_some_and(|readings| readings.iter().any(|(_, bpm)| bpm.is_some()))
    }
}

/// The day's wear, or [`None`] where the day will not read: no evidence
/// either way.
///
/// **Dated by the day Garmin files it under**, which is the date it was asked
/// for.
fn wear(record: &LandedRecord) -> Option<WearDay> {
    let day: Date = record.source_record_id().as_str().parse().ok()?;
    let served: Day = serde_json::from_slice(record.payload().as_bytes()).ok()?;
    Some(WearDay {
        day,
        worn: served.worn(),
        landed_as: record.id(),
        source_record_id: record.source_record_id().clone(),
    })
}

/// Garmin's adapter for the wrist's day: resting heart rate, and wear beside it.
#[derive(Debug, Clone, Copy, Default)]
pub struct GarminDailyHeartRateTranslator;

impl Translator for GarminDailyHeartRateTranslator {
    type Account = DayAccount;
    type Entity = GarminDayEntity;

    fn translate(
        &self,
        account: &DayAccount,
        zones: &StatedZones,
    ) -> Result<Translation<GarminDayEntity>, NormalisationError> {
        let resting = GarminRestingHeartRateTranslator.translate(account, zones)?;
        let wear = wear(account.day()).map(GarminDayEntity::Wear);
        Ok(match (resting, wear) {
            (Translation::Retraction { of }, _) => Translation::Retraction { of },
            (Translation::Entity { entity, refusals }, wear) => Translation::Entities {
                entities: NonEmpty::of(
                    GarminDayEntity::RestingHeartRate(*entity),
                    wear.into_iter().collect(),
                ),
                refusals,
            },
            (Translation::Entities { entities, refusals }, wear) => {
                let first = GarminDayEntity::RestingHeartRate(entities.first().clone());
                let rest = entities
                    .into_vec()
                    .into_iter()
                    .skip(1)
                    .map(GarminDayEntity::RestingHeartRate)
                    .chain(wear)
                    .collect();
                Translation::Entities {
                    entities: NonEmpty::of(first, rest),
                    refusals,
                }
            }
            (Translation::Refused(refusals), None) => Translation::Refused(refusals),
            (Translation::Refused(refusals), Some(wear)) => Translation::Entities {
                entities: NonEmpty::of(wear, Vec::new()),
                refusals: refusals.into_vec(),
            },
        })
    }
}
