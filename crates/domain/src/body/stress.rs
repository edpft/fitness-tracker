//! Garmin's stress: one average a day, from the wrist (#517, #777).
//!
//! **Here beside [`super::hrv`], for its reason**: the watch works the score
//! out from heart rate and its variability by an algorithm of its own (§ 6),
//! so another watch's days are another series.
//!
//! **The day's average, and how much of the day it was measured over.** The
//! operator ruled the score shown, 2026-11-11, and read it as the whole day's
//! average on the mocks he signed off. How long it was measured for is kept
//! beside it because a day the watch was worn for a morning is not a day's
//! average, and which days count is the analytical layer's to say.
//!
//! **Read off the operator's 3,232 landed days** (2018-01-06 to 2026-11-11):
//! 2,679 state an average, 2,350 of them measured over 20 hours or more, and
//! 553 state none, refused as
//! [`crate::normalised::RefusalReason::NoStressRecorded`]. Every day that
//! states one is a day the watch was worn. Garmin's own stress stream states
//! the same average on every day.
//!
//! **What stays in raw**: the highest reading, the time in each of Garmin's
//! bands and its qualifier for the day, and the reading every three minutes,
//! which the stress stream holds only since 2026-06-01.

use std::fmt;

use jiff::civil::Date;

use crate::device::DeviceIdentifier;
use crate::{
    landing::{LandingRecordId, Provenance, SourceRecordId},
    measure::PositiveDuration,
    normalised::NormalisedEntity,
};

/// Not a stress level: outside 1 to 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a stress level is 1 to 100, and this is {0}")]
pub struct InvalidStressLevel(i64);

/// Garmin's stress score, 1 to 100, lower being a body more at rest.
///
/// **It cannot hold what Garmin serves for no measurement**, −1 and −2, for
/// [`crate::measure::BeatsPerMinute`]'s reason: the absence of a reading is
/// not a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StressLevel(std::num::NonZeroU8);

impl StressLevel {
    const HIGHEST: i64 = 100;

    /// # Errors
    ///
    /// [`InvalidStressLevel`] outside 1 to 100.
    pub fn new(level: i64) -> Result<Self, InvalidStressLevel> {
        u8::try_from(level)
            .ok()
            .filter(|_| level <= Self::HIGHEST)
            .and_then(std::num::NonZeroU8::new)
            .map(Self)
            .ok_or(InvalidStressLevel(level))
    }

    pub const fn as_u8(self) -> u8 {
        self.0.get()
    }
}

impl fmt::Display for StressLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Everything one day's record says, on its way into [`Stress`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StressRecord {
    /// The day Garmin reports it under, as the watch's calendar has it.
    pub day: Date,
    pub average: StressLevel,
    /// How much of the day Garmin measured stress over.
    pub measured: PositiveDuration,
    pub landed_as: LandingRecordId,
    pub source_record_id: SourceRecordId,
    pub provenance: Provenance,
    /// The watch Garmin's sleep record names for the night, where it names
    /// one (#569): every night since 2024-12-22.
    pub recorded_by: Option<DeviceIdentifier>,
}

/// One day's stress, as Garmin told it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stress {
    day: Date,
    average: StressLevel,
    measured: PositiveDuration,
    landed_as: LandingRecordId,
    source_record_id: SourceRecordId,
    provenance: Provenance,
    recorded_by: Option<DeviceIdentifier>,
}

impl Stress {
    pub fn new(record: StressRecord) -> Self {
        Self {
            day: record.day,
            average: record.average,
            measured: record.measured,
            landed_as: record.landed_as,
            source_record_id: record.source_record_id,
            provenance: record.provenance,
            recorded_by: record.recorded_by,
        }
    }

    pub const fn day(&self) -> Date {
        self.day
    }

    pub const fn average(&self) -> StressLevel {
        self.average
    }

    pub const fn measured(&self) -> PositiveDuration {
        self.measured
    }

    /// The watch that recorded it, where Garmin's records name one.
    pub const fn recorded_by(&self) -> Option<&DeviceIdentifier> {
        self.recorded_by.as_ref()
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

impl NormalisedEntity for Stress {
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.source_record_id]
    }
}

impl fmt::Display for Stress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — stress {}, measured over {}",
            self.day, self.average, self.measured
        )
    }
}

#[cfg(test)]
mod tests {
    use super::StressLevel;

    #[test]
    fn a_level_is_one_to_a_hundred() {
        assert_eq!(StressLevel::new(28).map(StressLevel::as_u8), Ok(28));
        assert_eq!(StressLevel::new(100).map(StressLevel::as_u8), Ok(100));
        for not_a_level in [-2, -1, 0, 101, 300] {
            assert!(StressLevel::new(not_a_level).is_err(), "{not_a_level}");
        }
    }
}
