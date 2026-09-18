//! Turning one night of Garmin's HRV into an [`OvernightHrv`].
//!
//! Deterministic and total, as the other translators are (§ 9). One landing
//! record is one night, so there is nothing to compose: this is § II.3.1's
//! degenerate case.
//!
//! **Four things are refused, each read off the operator's 634 nights (#161)**:
//!
//! - a night with no baseline, which is the 18 Garmin served while it gathered
//!   three weeks of sleep and answered `status: "NONE"` — the operator's ruling,
//!   2026-09-18: *"without a baseline, you can't actually report HRV"*;
//! - a night Garmin measured nothing on, which it serves as a summary with no
//!   average, no five-minute high and a window two seconds long — 23 of them;
//! - a night with a five-minute high and no average, which is one night,
//!   2026-03-12, and is refused with its own reason so it stays distinguishable
//!   from the 23;
//! - a `status` string outside our vocabulary, which is a gap in the mapping
//!   rather than a fact about the night (§ 8).
//!
//! None is dropped (§ 37), and none of the four is a run failure: every one is a
//! [`Refusal`] inside a successful translation.

use application::{NormalisationError, Translation, ports::Translator};
use domain::{
    body::{
        HrvBaseline, HrvReading, HrvStatus, LastNight, MeasurementWindow, OvernightHrv,
        OvernightHrvRecord, WeeklyStatus,
    },
    landing::{EventKind, LandedRecord, Provenance},
    measure::HeartRateVariability,
    normalised::{OperatorZone, RefusalLocus, RefusalReason, StartedAt},
    sequence::NonEmpty,
};
use jiff::civil::DateTime;
use serde::Deserialize;

use crate::scribe::Scribe;

use super::account::NightAccount;

/// What Garmin says while it has not enough history to state a status.
const NO_STATUS: &str = "NONE";

/// Garmin's three positions, and ours.
const BALANCED: &str = "BALANCED";
const UNBALANCED: &str = "UNBALANCED";
const LOW: &str = "LOW";

/// A night Garmin measured nothing on.
const NOTHING_MEASURED: &str = "overnight reading of any kind";
/// A night measured, but with no average stated for it.
const NO_AVERAGE: &str = "overnight average";

#[derive(Debug, Deserialize)]
struct Night {
    #[serde(rename = "hrvSummary")]
    summary: Option<Summary>,
    #[serde(default, rename = "hrvReadings")]
    readings: Vec<Reading>,
    #[serde(rename = "startTimestampGMT")]
    start: Option<String>,
    #[serde(rename = "endTimestampGMT")]
    end: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Summary {
    #[serde(rename = "weeklyAvg")]
    weekly_average: Option<u32>,
    #[serde(rename = "lastNightAvg")]
    last_night_average: Option<u32>,
    #[serde(rename = "lastNight5MinHigh")]
    five_minute_high: Option<u32>,
    baseline: Option<Baseline>,
    status: String,
}

#[derive(Debug, Deserialize)]
struct Baseline {
    #[serde(rename = "lowUpper")]
    low_upper: Option<u32>,
    #[serde(rename = "balancedLow")]
    balanced_low: Option<u32>,
    #[serde(rename = "balancedUpper")]
    balanced_upper: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct Reading {
    #[serde(rename = "hrvValue")]
    value: Option<u32>,
    #[serde(rename = "readingTimeGMT")]
    taken_at: String,
}

/// A variability figure, or why it is not one.
fn variability(value: u32, field: &'static str) -> Result<HeartRateVariability, RefusalReason> {
    HeartRateVariability::from_milliseconds(value).map_err(|error| RefusalReason::UnreadableValue {
        field,
        detail: error.to_string(),
    })
}

/// A naive GMT stamp, placed. Garmin serves no offset, and every one of these is
/// UTC — the wall clock beside it is the watch's and is not carried (§ II.3).
fn instant(stamp: &str, field: &'static str) -> Result<jiff::Timestamp, RefusalReason> {
    let unreadable = |detail: String| RefusalReason::UnreadableValue { field, detail };
    let civil: DateTime = stamp.parse().map_err(|error: jiff::Error| {
        unreadable(format!("{stamp:?} is not a date and time: {error}"))
    })?;
    civil
        .to_zoned(jiff::tz::TimeZone::UTC)
        .map(|zoned| zoned.timestamp())
        .map_err(|error| unreadable(format!("{stamp:?} is not an instant: {error}")))
}

fn started_at(
    stamp: &str,
    field: &'static str,
    zone: &OperatorZone,
) -> Result<StartedAt, RefusalReason> {
    instant(stamp, field).map(|instant| StartedAt::new(instant, zone.clone()))
}

fn status(stated: &str) -> Result<HrvStatus, RefusalReason> {
    match stated {
        BALANCED => Ok(HrvStatus::Balanced),
        UNBALANCED => Ok(HrvStatus::Unbalanced),
        LOW => Ok(HrvStatus::Low),
        other => Err(RefusalReason::Unmodelled {
            detail: format!("HRV status {other:?}"),
        }),
    }
}

/// The three bounds, whole or refused.
fn baseline(stated: &Baseline) -> Result<HrvBaseline, RefusalReason> {
    let missing = |bound: &'static str| RefusalReason::UnreadableValue {
        field: "baseline",
        detail: format!("no {bound} bound"),
    };
    Ok(HrvBaseline {
        low_upper: variability(
            stated.low_upper.ok_or_else(|| missing("lower"))?,
            "baseline",
        )?,
        balanced_low: variability(
            stated.balanced_low.ok_or_else(|| missing("balanced low"))?,
            "baseline",
        )?,
        balanced_upper: variability(
            stated
                .balanced_upper
                .ok_or_else(|| missing("balanced upper"))?,
            "baseline",
        )?,
    })
}

fn readings(
    served: Vec<Reading>,
    zone: &OperatorZone,
) -> Result<Option<NonEmpty<HrvReading>>, RefusalReason> {
    let mut readings = Vec::with_capacity(served.len());
    for reading in served {
        let taken_at = started_at(&reading.taken_at, "reading time", zone)?;
        let value = variability(
            reading
                .value
                .ok_or_else(|| RefusalReason::UnreadableValue {
                    field: "reading",
                    detail: "a reading with no value".to_owned(),
                })?,
            "reading",
        )?;
        readings.push(HrvReading { taken_at, value });
    }
    Ok(NonEmpty::new(readings).ok())
}

/// The night, or the one reason there is none.
fn overnight_hrv(
    record: &LandedRecord,
    zone: &OperatorZone,
) -> Result<OvernightHrv, RefusalReason> {
    let night: Night = serde_json::from_slice(record.payload().as_bytes()).map_err(|error| {
        RefusalReason::UnreadablePayload {
            detail: error.to_string(),
        }
    })?;

    let summary = night
        .summary
        .ok_or_else(|| RefusalReason::UnreadablePayload {
            detail: "a night with no hrvSummary".to_owned(),
        })?;

    // **The baseline first**, because a night without one is not a night with
    // HRV and its own figures are beside the point.
    let Some(stated_baseline) = summary.baseline else {
        // A status other than `NONE` with no baseline is not a shape the
        // operator's 634 nights hold, so it is refused as its own thing rather
        // than counted with his ruling's 18.
        return Err(if summary.status == NO_STATUS {
            RefusalReason::WithoutBaseline
        } else {
            RefusalReason::Unmodelled {
                detail: format!("a night with status {:?} and no baseline", summary.status),
            }
        });
    };

    let average = summary
        .last_night_average
        .ok_or_else(|| RefusalReason::MissingFigure {
            figure: if summary.five_minute_high.is_none() {
                NOTHING_MEASURED
            } else {
                NO_AVERAGE
            },
        })?;
    // Neither of these is reachable while the average is stated — no night in the
    // operator's record has one without the other two — so both are refused with
    // the same reason as the average rather than with names of their own.
    let five_minute_high = summary
        .five_minute_high
        .ok_or(RefusalReason::MissingFigure { figure: NO_AVERAGE })?;
    let weekly_average = summary
        .weekly_average
        .ok_or(RefusalReason::MissingFigure { figure: NO_AVERAGE })?;

    let (Some(start), Some(end)) = (night.start, night.end) else {
        return Err(RefusalReason::UnreadablePayload {
            detail: "a night with no measurement window".to_owned(),
        });
    };
    let measured = MeasurementWindow::new(
        started_at(&start, "window start", zone)?,
        started_at(&end, "window end", zone)?,
    )
    .map_err(|error| RefusalReason::UnreadableValue {
        field: "measurement window",
        detail: error.to_string(),
    })?;

    Ok(OvernightHrv::new(OvernightHrvRecord {
        measured,
        last_night: LastNight {
            average: variability(average, "last night's average")?,
            five_minute_high: variability(five_minute_high, "last night's five-minute high")?,
            readings: readings(night.readings, zone)?,
        },
        weekly: WeeklyStatus {
            average: variability(weekly_average, "the weekly average")?,
            baseline: baseline(&stated_baseline)?,
            status: status(&summary.status)?,
        },
        landed_as: record.id(),
        source_record_id: record.source_record_id().clone(),
        provenance: record.provenance().clone(),
    }))
}

/// The Garmin adapter's translator.
#[derive(Debug, Clone, Copy, Default)]
pub struct GarminHrvTranslator;

impl Translator for GarminHrvTranslator {
    type Account = NightAccount;
    type Entity = OvernightHrv;

    fn translate(
        &self,
        account: &NightAccount,
        zone: &OperatorZone,
    ) -> Result<Translation<OvernightHrv>, NormalisationError> {
        let record = account.night();
        let mut scribe = Scribe::new(record);

        let Provenance::Event(event) = record.provenance();
        match event.kind() {
            // The endpoint serves a night as it now stands and reports no
            // deletion. Were it to, the night goes with the record.
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

        Ok(match overnight_hrv(record, zone) {
            Ok(night) => Translation::Entity {
                entity: Box::new(night),
                refusals: scribe.into_refusals(),
            },
            Err(reason) => scribe.only(RefusalLocus::Record, reason),
        })
    }
}
