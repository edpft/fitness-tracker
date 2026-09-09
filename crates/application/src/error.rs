//! The application's view of failure.
//!
//! Adapters translate at the boundary: no HTTP status, no SQL code, no
//! `reqwest::Error` and no `sqlx::Error` appears here or anywhere inward. A
//! detail string is not a vendor type — it carries what a human needs without
//! letting a technology choice ripple in.

use domain::landing::FailureReason;
use domain::plan::{PlanName, PlanWindow};
use jiff::civil::Date;

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
/// mesocycle, a date the mesocycle does not run, or an unavailable store stops
/// generation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PrescriptionError {
    #[error(transparent)]
    Store(#[from] StoreError),

    /// No authored mesocycle covers this date.
    ///
    /// Two states wear one variant, and the message tells them apart: nothing
    /// authored at all — the ordinary first-run case — and a date that falls in
    /// a gap between two mesocycles, before the first, or after the last.
    /// Neither is a fault, and the CLI has something helpful to say about both.
    /// A provided cycle opening from the one before it, whose predecessor has
    /// measured nothing.
    ///
    /// **Refused rather than opened from the authored number**, because there is
    /// no authored number: a cycle that inherits states none. The week-4
    /// one-repetition maximum before it is what it opens from, and until that is
    /// in the record there is nothing to be a share of. Recording the test
    /// answers it.
    #[error(
        "the cycle beginning {start} opens from the one before it, and that one \
         has not measured a maximum yet — record its test"
    )]
    NoInheritedMaximum { start: jiff::civil::Date },

    #[error("no plan covers {date}, so there is nothing to prescribe for it")]
    NoPlan { date: Date },

    /// Two plans would answer for the same day.
    ///
    /// Refused at authoring, because which of them answered would otherwise
    /// depend on the order rows came back in.
    ///
    /// **Plans, not programmes.** The gym and the bike inside one plan compete
    /// for every day of it on purpose; what may not compete is two plans, which
    /// is what a name identifies. Versions of one plan are the same plan
    /// re-authored and never conflict.
    #[error("{proposed} overlaps {existing}, and two plans may not answer for one day")]
    OverlappingPlan {
        proposed: PlanWindow,
        existing: PlanWindow,
    },

    /// A plan exists but the § 14 parameters it reads do not.
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
    InconsistentMesocycle(#[from] domain::prescription::InconsistentMesocycle),

    /// The programme's plan cannot be built from the parameters in force —
    /// most often because no load scale has been authored for the implement the
    /// primary is lifted on.
    #[error(transparent)]
    NoLadder(#[from] domain::prescription::InvalidLadder),

    /// Every slot failed to derive, so there is no workout to issue.
    ///
    /// Distinct from a slot or two being underivable, which is a value on the
    /// result: a prescription with ten good slots and one gap is worth issuing,
    /// and one with no slots at all is not a prescription.
    #[error("no slot could be derived, so there is no workout to issue")]
    NothingDerivable,
    /// A block claiming to open from an earlier test of its lift, where no such
    /// test ran.
    ///
    /// **What makes decision 0013's table a rule.** Without it the operator could
    /// open a block on a lift they have never tested by writing `provenance =
    /// "tested"` beside a number — the evasion 0013 named and, until this check,
    /// only described. It refuses the claim rather than the choice: a block with
    /// nothing to inherit may run its own entry test or declare a number.
    #[error(
        "the block starting {start} in {plan} opens from a tested {primary} \
         maximum, and {} produced no {primary} maximum for it to open from",
        predecessor.map_or_else(
            || "nothing before it".to_owned(),
            |date| format!("the mesocycle before it, starting {date},")
        )
    )]
    NoMaximumToOpenFrom {
        plan: PlanName,
        start: jiff::civil::Date,
        primary: &'static str,
        predecessor: Option<jiff::civil::Date>,
    },
    /// The maximum exists in the right lift, and the anchor is not dated to it.
    ///
    /// **The other half of "opens from a maximum that exists".** A date inside
    /// the predecessor is what makes the number that programme's result rather
    /// than one the operator wrote down beside its name.
    #[error(
        "the block starting {start} in {plan} opens from a maximum dated \
         {tested}, which is not a day the mesocycle starting {predecessor} ran \
         — so it is not that mesocycle's result"
    )]
    MaximumIsNotTheOneBefore {
        plan: PlanName,
        start: jiff::civil::Date,
        tested: jiff::civil::Date,
        predecessor: jiff::civil::Date,
    },
    /// The maximum exists, and is too old to speak for this programme.
    #[error(
        "the block in {plan} opens from a maximum measured on {tested}, and a \
         block starting {start} takes one from the {weeks} weeks before it"
    )]
    MaximumIsStale {
        plan: PlanName,
        tested: jiff::civil::Date,
        start: jiff::civil::Date,
        weeks: i64,
    },
    /// A test whose target is inherited, with nothing before it to inherit from.
    ///
    /// Refused rather than issued with one slot missing. A test week's whole
    /// purpose is the attempt, and a session that cannot say what the attempt is
    /// at is not a diminished test but a week that does not answer.
    #[error(
        "the test starting {start} in {plan} takes its target from the \
         mesocycle before it, and there is no such mesocycle in the same lift"
    )]
    NoTarget {
        plan: PlanName,
        start: jiff::civil::Date,
    },

    /// The block in force has no session left at or after the date asked from.
    ///
    /// Distinct from [`Self::NoPlan`]: a mesocycle covers the day and simply has
    /// nothing more to run, which is a block that has finished rather than a gap
    /// in the plan. The operator's answer is a new plan, not a correction.
    #[error("the programme has no session left on or after {from}")]
    NoSessionScheduled { from: Date },

    /// No operator time zone is declared. The same gap as
    /// [`NormalisationError::MissingTimeZone`], and it bites harder here: the
    /// zone decides which calendar day "the next session" falls on.
    #[error("no operator time zone is declared, so no date can be placed in a block")]
    MissingTimeZone,
}

/// What can go wrong comparing a performance against its prescription.
#[derive(Debug, thiserror::Error)]
pub enum ComparisonError {
    #[error(transparent)]
    Store(#[from] StoreError),

    /// Nothing was prescribed for the date, so there is nothing to compare
    /// against. Deliberately not "so I derived one": a comparison is a reading
    /// of the record, and issuing a prescription to have something to compare
    /// with would invent the expectation it is meant to be testing.
    #[error("nothing was prescribed for {date}, so there is nothing to compare against")]
    NothingIssued { date: Date },

    /// A session was prescribed and nothing answers it: no performance names it,
    /// and nothing was trained that day.
    #[error("the session prescribed for {date} has not been performed")]
    NotPerformed { date: Date },

    /// More than one session was trained on the day and none of them names the
    /// prescription, so which one answered it is not something the record says.
    ///
    /// Refused rather than resolved by picking the first: a comparison run
    /// against the wrong workout reports divergences that are really a mismatch,
    /// which is worse than declining to answer.
    #[error(
        "{count} sessions were trained on {date} and none names the prescription, \
         so which one answered it is not recorded"
    )]
    AmbiguousDay { date: Date, count: usize },
}

/// Why a prescription could not be put where the operator trains from.
///
/// Separate from [`PrescriptionError`] because the two fail for unrelated
/// reasons and an operator acts on them differently: a session that cannot be
/// *derived* is a programme or a record problem, and one that cannot be
/// *delivered* is a network or a credential problem with a perfectly good
/// prescription sitting in the store behind it. § 36 turns on that distinction —
/// the destination being unreachable degrades the system and never fails it.
#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error(transparent)]
    Store(#[from] StoreError),

    /// Nothing has been issued for the date, so there is nothing to deliver.
    ///
    /// Deliberately not "so I issued one": deriving a session is a decision that
    /// advances a ladder and writes to the record, and doing it as a side effect
    /// of a delivery would hide it.
    #[error("nothing is issued for {date}, so there is nothing to deliver")]
    NothingIssued { date: Date },

    /// The prescription is issued but its mesocycle is gone, so the session
    /// cannot be placed in a block. Corrupt rather than ordinary.
    #[error("the prescription for {date} names a mesocycle that is not in the store")]
    NoMesocycle { date: Date },

    /// The destination is unreachable, refused our credential, or rejected what
    /// we sent it.
    #[error("{destination} would not take the session: {message}")]
    Unreachable {
        destination: String,
        message: String,
    },

    /// The destination answered, but with something we cannot read as a
    /// delivery — no identifier, or one we cannot use.
    #[error("{destination} accepted the session but did not say what it called it: {message}")]
    Unidentifiable {
        destination: String,
        message: String,
    },

    /// A replacement was aimed at a session the destination no longer holds.
    ///
    /// **Not repaired by creating one.** The store says a place is occupied and
    /// the destination says it is not, and the two disagreeing is the fact worth
    /// surfacing: the operator deleted the routine by hand, or it was never
    /// there. Delivering a new one would resolve the disagreement by
    /// overwriting the evidence of it, and would leave the store's reference
    /// pointing at nothing either way.
    #[error(
        "{destination} no longer holds {reference}, which the store records as \
         the session for {date}"
    )]
    Vanished {
        destination: String,
        reference: String,
        date: Date,
    },

    /// A delivery that failed after the destination had answered, wrapping what
    /// it said.
    ///
    /// **The reply is shown as well as kept** (#124). What the operator used to
    /// see was serde's complaint about a column number — *"invalid type: map,
    /// expected a sequence at line 1 column 11"* — with the body that would have
    /// explained it already dropped, so column 11 had to be reasoned back into
    /// `{"routine":` rather than read. The bytes are in the store either way;
    /// printing them is what makes the surprise legible at the moment it
    /// happens, which is the operator's rule: *"if we get something we don't
    /// expect, we should print it as an error"*.
    ///
    /// Wraps rather than replaces, so an unreachable destination still reads as
    /// unreachable and a vanished routine still names what vanished.
    #[error(
        "{inner}\n  {destination} answered {status} with: {body}\n  the same reply is kept in \
         `delivery_reply`, against prescription {prescription} at {destination}"
    )]
    Answered {
        inner: Box<Self>,
        destination: String,
        prescription: i64,
        status: String,
        body: String,
    },
}
