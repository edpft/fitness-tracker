//! One invocation of extraction: when it started, what it collected, and
//! whether it finished.

use std::fmt;

use super::{ids::LandingStream, time::FetchedAt};

/// A stored run id that could not be one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a run id cannot be negative, found {value}")]
pub struct NegativeRunId {
    pub value: i64,
}

/// The store's identifier for a run.
///
/// Unsigned, because a position in the store's own sequence has no negative
/// values. SQLite counts in `i64`, so the narrowing happens once at the store
/// adapter — where a negative id means the file holds something this program
/// did not write, and says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunId(u64);

impl RunId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl From<u64> for RunId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl TryFrom<i64> for RunId {
    type Error = NegativeRunId;

    fn try_from(id: i64) -> Result<Self, Self::Error> {
        u64::try_from(id)
            .map(Self)
            .map_err(|_| NegativeRunId { value: id })
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How many events a run was served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct EventCount(u64);

impl EventCount {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl From<u64> for EventCount {
    fn from(count: u64) -> Self {
        Self(count)
    }
}

impl fmt::Display for EventCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How many landing records a run wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct RecordCount(u64);

impl RecordCount {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl From<u64> for RecordCount {
    fn from(count: u64) -> Self {
        Self(count)
    }
}

impl fmt::Display for RecordCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A stored reason this version of the program does not know.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not a failure reason this version knows")]
pub struct UnknownFailureReason {
    pub value: String,
}

/// Why a run did not finish.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FailureReason {
    #[error("the source was unreachable")]
    SourceUnavailable,
    #[error("the source rejected our credential")]
    Unauthorised,
    #[error("another run is already in progress")]
    AlreadyRunning,
    #[error("the source served an event carrying no identifier")]
    MissingProvenance,
    #[error("the store was unavailable")]
    StoreFailure,
    #[error("the source served a response we could not read")]
    MalformedResponse,
}

impl FailureReason {
    /// The stored form. Round-trips through `TryFrom<&str>`.
    ///
    /// Not `Display`, which is the sentence an operator reads.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SourceUnavailable => "source_unavailable",
            Self::Unauthorised => "unauthorised",
            Self::AlreadyRunning => "already_running",
            Self::MissingProvenance => "missing_provenance",
            Self::StoreFailure => "store_failure",
            Self::MalformedResponse => "malformed_response",
        }
    }
}

/// Read a reason back from the store.
///
/// A value we do not know is a reason recorded by a version of this program
/// that knew something this one does not. It is an error here so that the
/// caller decides — and the store's caller keeps the run's failure while
/// discarding only the why, because losing the fact that it failed would be
/// worse.
impl TryFrom<&str> for FailureReason {
    type Error = UnknownFailureReason;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "source_unavailable" => Ok(Self::SourceUnavailable),
            "unauthorised" => Ok(Self::Unauthorised),
            "already_running" => Ok(Self::AlreadyRunning),
            "missing_provenance" => Ok(Self::MissingProvenance),
            "store_failure" => Ok(Self::StoreFailure),
            "malformed_response" => Ok(Self::MalformedResponse),
            other => Err(UnknownFailureReason {
                value: other.to_owned(),
            }),
        }
    }
}

impl std::str::FromStr for FailureReason {
    type Err = UnknownFailureReason;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value)
    }
}

/// What became of a run.
///
/// A sum type rather than a status field beside optional counts, so a run
/// cannot be both succeeded and failed, and cannot report what it landed
/// without having finished.
///
/// The distinction between `events_seen` and `records_landed` is what makes a
/// silent failure visible: a run that saw 40 events and landed none found
/// nothing new, a run that saw none found nothing at all, and neither is a
/// failure. All three read differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    InFlight,
    Succeeded {
        finished_at: FetchedAt,
        events_seen: EventCount,
        records_landed: RecordCount,
    },
    Failed {
        finished_at: FetchedAt,
        reason: FailureReason,
    },
}

impl RunOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded { .. })
    }

    pub fn finished_at(&self) -> Option<FetchedAt> {
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
pub struct ExtractionRun {
    id: RunId,
    stream: LandingStream,
    started_at: FetchedAt,
    outcome: RunOutcome,
}

impl ExtractionRun {
    pub fn new(
        id: RunId,
        stream: LandingStream,
        started_at: FetchedAt,
        outcome: RunOutcome,
    ) -> Self {
        Self {
            id,
            stream,
            started_at,
            outcome,
        }
    }

    pub fn id(&self) -> RunId {
        self.id
    }

    pub fn stream(&self) -> &LandingStream {
        &self.stream
    }

    pub fn started_at(&self) -> FetchedAt {
        self.started_at
    }

    pub fn outcome(&self) -> &RunOutcome {
        &self.outcome
    }
}
