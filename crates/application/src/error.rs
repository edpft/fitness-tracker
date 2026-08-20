//! The application's view of failure.
//!
//! Adapters translate at the boundary: no HTTP status, no SQL code, no
//! `reqwest::Error` and no `sqlx::Error` appears here or anywhere inward. A
//! detail string is not a vendor type — it carries what a human needs without
//! letting a technology choice ripple in.

use domain::landing::FailureReason;

/// A source did not give us what we asked for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    /// Unreachable, or reachable and failing, after the adapter exhausted its
    /// retries. Throttling ends up here too: the adapter backs off, and gives
    /// up only when backing off has stopped helping.
    #[error("the source was unreachable: {detail}")]
    Unavailable { detail: String },

    /// Terminal, and never retried. A rejected credential will not un-reject
    /// itself, and retrying looks like an attack.
    #[error("the source rejected our credential")]
    Unauthorised,

    /// A response we could not read, or one missing the provenance a landing
    /// record requires.
    #[error("the source served a response we could not read: {detail}")]
    Malformed { detail: String },
}

/// The store did not do what we asked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("the store was unavailable: {detail}")]
    Unavailable { detail: String },

    /// Something is in the store that this program did not put there, or could
    /// not have. Worth distinguishing: it does not get better on retry.
    #[error("the store holds something we cannot read: {detail}")]
    Corrupt { detail: String },
}

/// Another run holds the lock, or the lock itself is unusable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunLockError {
    #[error("another extraction run is already in progress")]
    Held,
    #[error("the run lock could not be taken: {detail}")]
    Unavailable { detail: String },
}

/// Why an extraction run did not complete.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExtractionError {
    /// Nothing was landed and the resumption point did not move.
    #[error("another extraction run is already in progress")]
    AlreadyRunning,

    #[error(transparent)]
    Source(#[from] SourceError),

    #[error(transparent)]
    Store(#[from] StoreError),

    /// The source served an event carrying no identifier. Landing it would
    /// produce a record that cannot say what it is about, so the run fails
    /// visibly instead — a record without provenance is worse than a loud
    /// failure.
    #[error("the source served an event carrying no identifier")]
    MissingProvenance,
}

impl ExtractionError {
    /// How this failure is recorded against the run, so that a later reader
    /// can tell what went wrong without the original error being in scope.
    pub const fn as_failure_reason(&self) -> FailureReason {
        match self {
            Self::AlreadyRunning => FailureReason::AlreadyRunning,
            Self::Source(SourceError::Unavailable { .. }) => FailureReason::SourceUnavailable,
            Self::Source(SourceError::Unauthorised) => FailureReason::Unauthorised,
            Self::Source(SourceError::Malformed { .. }) => FailureReason::MalformedResponse,
            Self::Store(_) => FailureReason::StoreFailure,
            Self::MissingProvenance => FailureReason::MissingProvenance,
        }
    }
}

impl From<RunLockError> for ExtractionError {
    fn from(error: RunLockError) -> Self {
        match error {
            RunLockError::Held => Self::AlreadyRunning,
            RunLockError::Unavailable { detail } => Self::Store(StoreError::Unavailable { detail }),
        }
    }
}

/// Reading or resetting extraction state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StatusError {
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Why a derivation of the normalised layer did not complete.
///
/// Note what is **not** here: any variant for bad data. That is the feature's
/// central distinction. A wrong record produces a `Refusal` and a successful
/// run, because refusing something a source served is the layer working rather
/// than failing. Only a defect in our own code stops a derivation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NormalisationError {
    #[error(transparent)]
    Store(#[from] StoreError),

    /// The exercise vocabulary has a gap. Naming the identifier is the whole
    /// point: the mapping is code (§ 9), so this is a defect to go and fix, and
    /// no workout containing the identifier translates around it.
    #[error("no exercise is mapped for template {template_id}, seen on record {source_record_id}")]
    UnmappedExercise {
        template_id: String,
        source_record_id: String,
    },

    /// No operator time zone is declared, so no timestamp can be built. § II.3
    /// takes the zone from configuration, and guessing one would make the
    /// derivation depend on the machine that ran it.
    #[error("no operator time zone is declared, so no workout can be given a wall clock")]
    MissingTimeZone,
}

/// Why a prescription could not be issued.
///
/// Note what is **not** here: any variant for a slot that could not be derived.
/// That is this feature's central distinction, and it mirrors the one
/// [`NormalisationError`] draws. A slot with no history to progress from is
/// reported as a value on the result, because a workout with ten good slots and
/// one gap is worth issuing and a refusal to answer is not. Only a missing
/// programme, a date the programme does not run, or an unavailable store stops
/// generation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrescriptionError {
    #[error(transparent)]
    Store(#[from] StoreError),

    /// Nothing has been authored yet. The ordinary first-run state, and the CLI
    /// has something helpful to say about it.
    #[error("no programme has been authored, so there is nothing to prescribe from")]
    NoProgramme,

    /// A programme exists but the § 14 parameters it reads do not.
    #[error("no generation parameters have been authored, so no load can be derived")]
    NoParameters,

    /// The date falls on no weekday the programme runs, before its start, or
    /// past its end.
    ///
    /// Declining names what the programme *does* run. There is deliberately no
    /// nearest-match: silently prescribing Friday's session for a Wednesday is
    /// worse than saying no.
    #[error(transparent)]
    NotScheduled(#[from] domain::prescription::NotScheduled),

    /// The authored programme is internally inconsistent in a way the types
    /// could not catch. Raised at authoring rather than at the first
    /// `prescribe`, so a bad programme never reaches the store.
    #[error(transparent)]
    InconsistentProgramme(#[from] domain::prescription::InconsistentProgramme),

    /// Every slot failed to derive, so there is no workout to issue.
    ///
    /// Distinct from a slot or two being underivable, which is a value on the
    /// result: a prescription with ten good slots and one gap is worth issuing,
    /// and one with no slots at all is not a prescription.
    #[error("no slot could be derived, so there is no workout to issue")]
    NothingDerivable,

    /// No operator time zone is declared. The same gap as
    /// [`NormalisationError::MissingTimeZone`], and it bites harder here: the
    /// zone decides which calendar day "the next session" falls on.
    #[error("no operator time zone is declared, so no date can be placed in a block")]
    MissingTimeZone,
}
