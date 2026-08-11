//! One invocation of extraction: when it started, what it collected, and
//! whether it finished.

use std::fmt;

use super::{ids::LandingStream, time::FetchedAt};

/// The store's identifier for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RunId(i64);

impl RunId {
    pub fn new(id: i64) -> Self {
        Self(id)
    }

    pub fn as_i64(self) -> i64 {
        self.0
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
    pub fn new(count: u64) -> Self {
        Self(count)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    #[must_use]
    pub fn increment(self) -> Self {
        Self(self.0.saturating_add(1))
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
    pub fn new(count: u64) -> Self {
        Self(count)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }

    #[must_use]
    pub fn increased_by(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
}

impl fmt::Display for RecordCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
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
    /// The stored form. Round-trips through [`FailureReason::parse`].
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

    /// Read a reason back from the store.
    ///
    /// An unknown value is not an error: it is a reason recorded by a version
    /// of this program that knew something this one does not, and losing the
    /// fact that the run failed would be worse than losing why.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "source_unavailable" => Some(Self::SourceUnavailable),
            "unauthorised" => Some(Self::Unauthorised),
            "already_running" => Some(Self::AlreadyRunning),
            "missing_provenance" => Some(Self::MissingProvenance),
            "store_failure" => Some(Self::StoreFailure),
            "malformed_response" => Some(Self::MalformedResponse),
            _ => None,
        }
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
