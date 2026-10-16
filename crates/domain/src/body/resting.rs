//! Garmin's resting heart rate: one figure a day, from the wrist (#254).
//!
//! **Here beside [`super::hrv`], for its reason**: a measurement of a body by
//! the instrument that took it. § 6 lists resting heart rate beside HRV as
//! method-dependent, so the figure is the watch's and its algorithm's, and a
//! figure from another watch is another series.
//!
//! **Read off the operator's 3,204 landed days** (2018-01-06 to 2026-10-14):
//! 2,654 state a resting heart rate and 550 state nothing at all, which are the
//! days nothing was worn and are refused as such. No day states readings
//! without a resting heart rate, or the reverse.
//!
//! **What stays in raw**: the day's readings, which Garmin serves only since
//! about 2026-05-15; its maximum and minimum, which are a function of them; and
//! `lastSevenDaysAvgRestingHeartRate`, which is Garmin averaging its own figure.

use std::fmt;

use jiff::civil::Date;

use crate::{
    landing::{LandingRecordId, Provenance, SourceRecordId},
    measure::BeatsPerMinute,
    normalised::NormalisedEntity,
};

/// Everything one day's record says, on its way into [`RestingHeartRate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestingHeartRateRecord {
    /// The day Garmin reports it under, as the watch's calendar has it.
    pub day: Date,
    pub rate: BeatsPerMinute,
    pub landed_as: LandingRecordId,
    pub source_record_id: SourceRecordId,
    pub provenance: Provenance,
}

/// One day's resting heart rate, as Garmin told it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestingHeartRate {
    day: Date,
    rate: BeatsPerMinute,
    landed_as: LandingRecordId,
    source_record_id: SourceRecordId,
    provenance: Provenance,
}

impl RestingHeartRate {
    pub fn new(record: RestingHeartRateRecord) -> Self {
        Self {
            day: record.day,
            rate: record.rate,
            landed_as: record.landed_as,
            source_record_id: record.source_record_id,
            provenance: record.provenance,
        }
    }

    pub const fn day(&self) -> Date {
        self.day
    }

    pub const fn rate(&self) -> BeatsPerMinute {
        self.rate
    }

    pub const fn landed_as(&self) -> LandingRecordId {
        self.landed_as
    }

    pub const fn source_record_id(&self) -> &SourceRecordId {
        &self.source_record_id
    }

    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }
}

impl NormalisedEntity for RestingHeartRate {
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.source_record_id]
    }
}

impl fmt::Display for RestingHeartRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} — {}", self.day, self.rate)
    }
}
