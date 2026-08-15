//! One invocation of the derivation: when it started, what it produced, and
//! whether it finished.
//!
//! Deliberately a parallel of `landing::run` rather than a widening of it.
//! Extraction and derivation fail for different reasons and report different
//! numbers — one counts what a source served, the other counts what our own
//! translation made of it — and a shared type would have to carry both sets of
//! fields with half of them empty in either direction.
//!
//! What they share is the reason for existing: § 38 wants a broken derivation
//! visible rather than merely absent, and one that found nothing must read
//! differently from one that failed.

use std::fmt;

use crate::landing::{FetchedAt, LandingStream, NegativeRunId, RecordCount};

/// The store's identifier for a derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NormalisationRunId(u64);

impl NormalisationRunId {
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

impl From<u64> for NormalisationRunId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl TryFrom<i64> for NormalisationRunId {
    type Error = NegativeRunId;

    fn try_from(id: i64) -> Result<Self, Self::Error> {
        u64::try_from(id)
            .map(Self)
            .map_err(|_| NegativeRunId { value: id })
    }
}

impl fmt::Display for NormalisationRunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How many gym workouts a derivation wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct WorkoutCount(usize);

impl WorkoutCount {
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl From<usize> for WorkoutCount {
    fn from(count: usize) -> Self {
        Self(count)
    }
}

impl fmt::Display for WorkoutCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How many refusals a derivation recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct RefusalCount(usize);

impl RefusalCount {
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl From<usize> for RefusalCount {
    fn from(count: usize) -> Self {
        Self(count)
    }
}

impl fmt::Display for RefusalCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A stored reason this version of the program does not know.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not a derivation failure reason this version knows")]
pub struct UnknownNormalisationFailure {
    pub value: String,
}

/// Why a derivation did not finish.
///
/// Two reasons, and neither of them is bad data. That is the point: a record
/// the domain refuses is recorded and stepped over, so the only things that can
/// stop a derivation are the store being unusable and our own vocabulary having
/// a hole in it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NormalisationFailure {
    #[error("the store was unavailable")]
    StoreFailure,
    #[error("an exercise template is not in the mapping")]
    UnmappedExercise,
    #[error("no operator time zone is declared")]
    MissingTimeZone,
}

impl NormalisationFailure {
    /// The stored form. Round-trips through `TryFrom<&str>`.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::StoreFailure => "store_failure",
            Self::UnmappedExercise => "unmapped_exercise",
            Self::MissingTimeZone => "missing_zone",
        }
    }
}

impl TryFrom<&str> for NormalisationFailure {
    type Error = UnknownNormalisationFailure;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "store_failure" => Ok(Self::StoreFailure),
            "unmapped_exercise" => Ok(Self::UnmappedExercise),
            "missing_zone" => Ok(Self::MissingTimeZone),
            other => Err(UnknownNormalisationFailure {
                value: other.to_owned(),
            }),
        }
    }
}

impl std::str::FromStr for NormalisationFailure {
    type Err = UnknownNormalisationFailure;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value)
    }
}

/// What became of a derivation.
///
/// A sum type for the reason `RunOutcome` is one: a derivation cannot be both
/// succeeded and failed, and cannot report what it produced without having
/// finished.
///
/// The counts on the success arm exist to add up: `records_read` equals
/// `workouts_written` plus `workouts_retracted` plus `retractions_read` plus
/// `records_refused`. Every record has exactly one outcome, so a record that
/// went missing shows up as arithmetic that does not reconcile.
///
/// The two retraction counts measure different things and neither implies the
/// other. `retractions_read` is how many withdrawal events the source served;
/// `workouts_retracted` is how many workouts those events actually removed. A
/// retraction naming a record that was never landed removes nothing, so a run
/// reporting one retraction and no retracted workout is reporting exactly that
/// rather than looking like a bug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalisationOutcome {
    InFlight,
    Succeeded {
        finished_at: FetchedAt,
        records_read: RecordCount,
        workouts_written: WorkoutCount,
        workouts_retracted: WorkoutCount,
        retractions_read: RecordCount,
        records_refused: RecordCount,
        refusals_recorded: RefusalCount,
    },
    Failed {
        finished_at: FetchedAt,
        reason: NormalisationFailure,
    },
}

impl NormalisationOutcome {
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded { .. })
    }

    pub const fn finished_at(&self) -> Option<FetchedAt> {
        match self {
            Self::InFlight => None,
            Self::Succeeded { finished_at, .. } | Self::Failed { finished_at, .. } => {
                Some(*finished_at)
            }
        }
    }
}

/// One invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalisationRun {
    id: NormalisationRunId,
    stream: LandingStream,
    started_at: FetchedAt,
    outcome: NormalisationOutcome,
}

impl NormalisationRun {
    pub const fn new(
        id: NormalisationRunId,
        stream: LandingStream,
        started_at: FetchedAt,
        outcome: NormalisationOutcome,
    ) -> Self {
        Self {
            id,
            stream,
            started_at,
            outcome,
        }
    }

    pub const fn id(&self) -> NormalisationRunId {
        self.id
    }

    pub const fn stream(&self) -> &LandingStream {
        &self.stream
    }

    pub const fn started_at(&self) -> FetchedAt {
        self.started_at
    }

    pub const fn outcome(&self) -> &NormalisationOutcome {
        &self.outcome
    }
}
