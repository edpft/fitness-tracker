//! A padel game (#517).
//!
//! **A padel game, not an activity** (operator, 2026-11-11: *"no, it's a
//! padel game"*). The watch files it in the same list as a gym session and a
//! ride, and that list is where it was found, not what it is.
//!
//! **It is training done, and nothing prescribes it.** A game occupies its
//! evening without being a slot ([`crate::schedule`]), and its hours count
//! towards what resting heart rate, overnight HRV and stress are read against
//! as a ride's and a gym session's do (operator, 2026-11-11: *"yes, all
//! activity hours"*).
//!
//! **When, how long, and the heart rate the watch stated over it.** The
//! operator's record holds 15, from 2026-03-22, every one recorded by the
//! Forerunner 165.

use std::fmt;

use crate::{
    landing::{LandingRecordId, Provenance, SourceRecordId},
    measure::{HeartRateSummary, PositiveDuration},
    normalised::{NormalisedEntity, StartedAt},
};

/// Everything one game's record says, on its way into [`PadelGame`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadelGameRecord {
    pub started_at: StartedAt,
    pub duration: PositiveDuration,
    /// Absent where the watch stated none, as a gym session's is.
    pub heart_rate: Option<HeartRateSummary>,
    pub landed_as: LandingRecordId,
    pub source_record_id: SourceRecordId,
    pub provenance: Provenance,
}

/// One game of padel, as the watch that recorded it told it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadelGame {
    started_at: StartedAt,
    duration: PositiveDuration,
    heart_rate: Option<HeartRateSummary>,
    landed_as: LandingRecordId,
    source_record_id: SourceRecordId,
    provenance: Provenance,
}

impl PadelGame {
    pub fn new(record: PadelGameRecord) -> Self {
        Self {
            started_at: record.started_at,
            duration: record.duration,
            heart_rate: record.heart_rate,
            landed_as: record.landed_as,
            source_record_id: record.source_record_id,
            provenance: record.provenance,
        }
    }

    pub const fn started_at(&self) -> &StartedAt {
        &self.started_at
    }

    pub const fn duration(&self) -> PositiveDuration {
        self.duration
    }

    pub const fn heart_rate(&self) -> Option<HeartRateSummary> {
        self.heart_rate
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

impl NormalisedEntity for PadelGame {
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.source_record_id]
    }
}

impl fmt::Display for PadelGame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — padel, {}",
            self.started_at.wall_clock().datetime(),
            self.duration
        )
    }
}
