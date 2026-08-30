//! The interfaces the application declares and the outer rings implement.
//!
//! Declared here, in the application's own vocabulary, so that a change of
//! HTTP client or store engine cannot ripple inwards. No `reqwest`, `sqlx`,
//! `serde` or `jiff` type appears in a signature.
//!
//! Every asynchronous method spells out `impl Future<…> + Send` rather than
//! using `async fn`. A bare `async fn` in a trait produces a future with no
//! `Send` bound, which cannot be held across an `await` on a multi-threaded
//! runtime or inside an HTTP handler. One line per method now is cheaper than
//! re-declaring every port when the `web` ring gains a surface.

use std::{collections::BTreeMap, future::Future};

use jiff::{Timestamp, civil::Date};

use domain::gym::{
    GymWorkout, Load, NonEmpty, NormalisationOutcome, NormalisationRun, NormalisationRunId,
    OperatorZone, Performed, Refusal, RefusalCount, RepCount, WorkoutCount, exercise::RepsExercise,
};
use domain::landing::{
    EventCount, ExtractionRun, FetchedAt, LandedRecord, LandingRecord, LandingRecordId,
    LandingStream, PayloadDigest, Provenance, RawPayload, RecordCount, RunId, RunOutcome,
    SourceRecordId, Watermark,
};
use domain::prescription::{
    GenerationParameters, PrescribedWorkout, PrescriptionState, Programme, ProgrammeId,
    ProgrammeName, ProgrammeWindow, Progress, SessionRole, SlotId,
};
use domain::schedule::{Alteration, Diary, TrainingPattern};

use crate::error::{
    DeliveryError, ExtractionError, NormalisationError, PrescriptionError, RunLockError,
    SourceError, StatusError, StoreError,
};

// --- Driven ports -----------------------------------------------------------

/// One record as the source served it, carrying its bytes intact.
///
/// The three things a landing record needs that only the source can supply.
/// How the source was asked, and what it called the shape it answered in, is
/// in the [`Provenance`] — which the adapter builds, because the adapter is
/// the only thing that knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEvent {
    pub source_record_id: SourceRecordId,
    pub provenance: Provenance,
    pub payload: RawPayload,
}

/// One instalment of a source's answer, in the order the source served it.
///
/// A source hands back what it is willing to hand back in one go; `resume`
/// says whether there is more and, if so, carries whatever the adapter needs
/// to pick up where it stopped. `None` means that was everything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventBatch<R> {
    pub events: Vec<SourceEvent>,
    pub resume: Option<R>,
}

/// Where observations come from.
pub trait WorkoutEventSource {
    /// Whatever the adapter needs to continue a walk it has already started.
    ///
    /// Opaque here on purpose. A page number, a cursor, an offset into a file
    /// and a continuation token are all the same thing to a run — the source's
    /// own way of saying "not finished" — and pagination is a fact about how a
    /// particular source answers, not about the data. The application counts
    /// records and stops when told to stop.
    type Resume: Send;

    /// The next instalment of everything served since `since`.
    ///
    /// `since` is inclusive at the source, so a caller passes its stored
    /// watermark unmodified: the boundary event is served again and
    /// deduplicated, which costs nothing and cannot skip a sibling sharing
    /// that timestamp. It is passed on every call rather than remembered, so
    /// the adapter holds no state between them.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if the source is unreachable, rejects our credential,
    /// or serves something unreadable.
    fn fetch(
        &self,
        since: Option<Watermark>,
        resume: Option<Self::Resume>,
    ) -> impl Future<Output = Result<EventBatch<Self::Resume>, SourceError>> + Send;
}

/// Raw landing for **one** stream.
///
/// Each landing table has its own instance, bound at construction, so there is
/// no stream argument to pass wrongly and a store for `hevy.workouts` cannot
/// read another stream's records.
pub trait LandingStore {
    /// Which stream this store holds.
    ///
    /// Asked rather than told, and this is the whole of how a run knows what
    /// it is extracting. Being bound to one table makes this store the only
    /// thing that can answer without being informed — so it is the single
    /// source of truth, and every other use of the stream in a run derives
    /// from it. A stream supplied alongside the ports could disagree with
    /// them; one read out of them cannot.
    fn stream(&self) -> &LandingStream;

    /// The digest of the most recent record for this source record, if there
    /// is one.
    ///
    /// Most recent, not any: a record edited to X, then Y, then back to X is
    /// the source serving three payloads, and all three are landed.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something
    /// unreadable.
    fn latest_digest(
        &self,
        id: &SourceRecordId,
    ) -> impl Future<Output = Result<Option<PayloadDigest>, StoreError>> + Send;

    /// Append records. Never updates, never deletes.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn append(
        &self,
        run: RunId,
        records: Vec<LandingRecord>,
    ) -> impl Future<Output = Result<RecordCount, StoreError>> + Send;

    /// How many records this stream holds in total. Reported by `status`.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn count(&self) -> impl Future<Output = Result<RecordCount, StoreError>> + Send;
}

/// Where a stream resumes from.
///
/// Reconstructible state: losing it costs a re-fetch, never a fact, which is
/// why `clear` is a supported operation rather than a repair.
pub trait ResumptionPointStore {
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn read(
        &self,
        stream: &LandingStream,
    ) -> impl Future<Output = Result<Option<Watermark>, StoreError>> + Send;

    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn advance(
        &self,
        stream: &LandingStream,
        to: Watermark,
        at: FetchedAt,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Discard the position, so the next run collects the full history.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn clear(&self, stream: &LandingStream) -> impl Future<Output = Result<(), StoreError>> + Send;
}

/// What happened on each invocation.
pub trait ExtractionRunLog {
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn begin(
        &self,
        stream: &LandingStream,
        at: FetchedAt,
    ) -> impl Future<Output = Result<RunId, StoreError>> + Send;

    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn finish(
        &self,
        run: RunId,
        outcome: RunOutcome,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// The most recent run that succeeded, so a silently broken extraction is
    /// visible rather than merely absent.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn latest_success(
        &self,
        stream: &LandingStream,
    ) -> impl Future<Output = Result<Option<ExtractionRun>, StoreError>> + Send;
}

/// Single-flight.
///
/// The guard releases on drop, and the kernel drops it if the process dies, so
/// a crashed run leaves nothing to unstick. State recorded in the store would
/// survive the crash and need a manual repair — a worse failure for a single
/// operator than a lock that simply lets go.
pub trait RunLock {
    /// `Send` because a run holds the guard across every await it makes: the
    /// lock covers the whole run, not just its opening. Requiring it here says
    /// so once, rather than at every use site.
    type Guard: Send;

    /// # Errors
    ///
    /// [`RunLockError::Held`] if another run has it. Fails immediately rather
    /// than waiting: two runs sharing a resumption point can advance it past
    /// records neither landed.
    fn try_acquire(&self, stream: &LandingStream) -> Result<Self::Guard, RunLockError>;
}

/// The current instant.
///
/// A port so that a run's timings are injectable and its behaviour testable
/// without sleeping. It is emphatically *not* how the resumption point is
/// set — that comes from event times a run observed.
pub trait Clock {
    fn now(&self) -> FetchedAt;
}

// --- Driving ports ----------------------------------------------------------
//
// Named for the thing that does the work rather than the work itself: a trait
// describes what an implementer *is*, and every implementer of these is a
// noun. The act is the method.

/// What a completed run is worth reporting.
///
/// `events_seen` and `records_landed` are both present and both reported: the
/// difference between them is what distinguishes a run that found nothing new
/// from one that found nothing at all, and neither from a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub run_id: RunId,
    pub events_seen: EventCount,
    pub records_landed: RecordCount,
    pub resumption_point: Option<Watermark>,
    pub resumption_point_moved: bool,
}

/// Where a stream stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamStatus {
    pub stream: LandingStream,
    pub last_success: Option<ExtractionRun>,
    pub records_held: RecordCount,
    pub resumption_point: Option<Watermark>,
}

// None of the three takes a stream. Each is built from ports that are already
// bound to one, and asks them which — see [`LandingStore::stream`]. A stream
// passed in here would be a second answer to a question that already has one,
// and two answers can disagree: a run whose lock, run log and resumption point
// name one stream while its records are tagged with another advances a
// watermark past events it never collected.

/// Collects everything the source has served since the resumption point.
pub trait WorkoutExtractor {
    /// # Errors
    ///
    /// [`ExtractionError`] if another run holds the lock, the source will not
    /// serve us, or the store will not take what it served.
    fn extract(&self) -> impl Future<Output = Result<RunSummary, ExtractionError>> + Send;
}

/// Reports where a stream stands.
pub trait ExtractionStatusReporter {
    /// # Errors
    ///
    /// [`StatusError`] if the store is unavailable.
    fn status(&self) -> impl Future<Output = Result<StreamStatus, StatusError>> + Send;
}

/// Discards a stream's resumption point.
pub trait ResumptionPointResetter {
    /// # Errors
    ///
    /// [`StatusError`] if the store is unavailable.
    fn reset(&self) -> impl Future<Output = Result<(), StatusError>> + Send;
}

// --- Normalisation ----------------------------------------------------------
//
// The second derivation's ports. They follow the same rules as the ones above:
// each store is bound to one stream and asked which, no vendor type appears in
// a signature, and nothing takes an overlay — § 9 forbids consulting one, and
// the strongest form of that is a port that could not be handed one.

/// What one landing record became.
///
/// The three outcomes a record can have, as a sum. A record that produced no
/// workout and no reason does not compile, and a retraction cannot carry
/// refusals — the two mistakes most worth making impossible, since either would
/// let a record go silently missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Translation {
    /// A workout, with whatever it would not accept listed beside it. A
    /// refusal inside a record does not stop the rest of it translating, so
    /// both travel together.
    Workout {
        workout: Box<GymWorkout>,
        refusals: Vec<Refusal>,
    },
    /// The source withdrew a record it previously served. Carries no refusals,
    /// because nothing was rejected.
    Retraction { of: SourceRecordId },
    /// Nothing translated, and here is why. Non-empty, so "no workout and no
    /// reason" is not a state that exists.
    Refused(NonEmpty<Refusal>),
}

/// Raw, read-only, for one stream.
///
/// A separate trait from [`LandingStore`] rather than a widening of it. That is
/// what makes "a derivation never writes to raw" a fact about the type rather
/// than a promise about the code: this reader has no `append`, so a derivation
/// holding one could not mutate an input if it tried.
pub trait LandingRecordReader {
    fn stream(&self) -> &LandingStream;

    /// Every record for this stream, oldest first, in the order the source
    /// served them.
    ///
    /// The order is defined so that a derivation is reproducible, not because
    /// the use case depends on it — retraction is absorbing and
    /// order-independent — and defining it is what lets that independence be
    /// tested by reversing the sequence.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something
    /// unreadable.
    fn records(&self) -> impl Future<Output = Result<Vec<LandedRecord>, StoreError>> + Send;
}

/// Turns one source's payload into our entity.
///
/// The one port that knows a source's format, and the only place a vendor's
/// shape is allowed to be understood. Synchronous and total: it makes no
/// request, reads no clock and consults no overlay, which is what
/// "deterministic translation" means and is visible here as a signature with
/// nothing to be non-deterministic with.
pub trait WorkoutTranslator {
    /// # Errors
    ///
    /// [`NormalisationError::UnmappedExercise`] and nothing else. A gap in our
    /// vocabulary is a defect in our code, so it stops the run — where every
    /// problem with the *data* becomes a `Refusal` inside a successful
    /// translation.
    fn translate(
        &self,
        record: &LandedRecord,
        zone: &OperatorZone,
    ) -> Result<Translation, NormalisationError>;
}

/// The normalised layer for one stream.
pub trait NormalisedWorkoutStore {
    fn stream(&self) -> &LandingStream;

    /// Replace this stream's normalised layer entirely.
    ///
    /// One transaction, and a replacement rather than an update: § II says a
    /// derivation is never mutated in place, and doing the full re-derivation
    /// every time is the cheapest way to be sure of it. There is no
    /// append-only trigger here as there is on raw — those guard an *input*,
    /// and applying them to a derivation would prevent the rebuild the
    /// constitution requires.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn replace(
        &self,
        run: NormalisationRunId,
        workouts: Vec<GymWorkout>,
    ) -> impl Future<Output = Result<WorkoutCount, StoreError>> + Send;

    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn count(&self) -> impl Future<Output = Result<WorkoutCount, StoreError>> + Send;
}

/// What the domain would not accept, for one stream.
pub trait RefusalStore {
    fn stream(&self) -> &LandingStream;

    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn replace(
        &self,
        run: NormalisationRunId,
        refusals: Vec<Refusal>,
    ) -> impl Future<Output = Result<RefusalCount, StoreError>> + Send;

    /// Read back after a derivation, so what the domain will not accept is
    /// visible rather than surfacing only in a log.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something
    /// unreadable.
    fn all(&self) -> impl Future<Output = Result<Vec<Refusal>, StoreError>> + Send;
}

/// What happened on each derivation.
///
/// Mirrors [`ExtractionRunLog`] exactly, for the same reason: § 38 wants a
/// broken derivation visible rather than merely absent, and a derivation that
/// found nothing must be distinguishable from one that failed.
pub trait NormalisationRunLog {
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn begin(
        &self,
        stream: &LandingStream,
        at: FetchedAt,
    ) -> impl Future<Output = Result<NormalisationRunId, StoreError>> + Send;

    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn finish(
        &self,
        run: NormalisationRunId,
        outcome: NormalisationOutcome,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn latest_success(
        &self,
        stream: &LandingStream,
    ) -> impl Future<Output = Result<Option<NormalisationRun>, StoreError>> + Send;
}

/// What a derivation did.
///
/// Numbers that must add up: `records_read` equals `workouts_written` plus
/// `workouts_retracted` plus `retractions_read` plus `records_refused`, so
/// a record going missing is visible without reading a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalisationSummary {
    pub run_id: NormalisationRunId,
    pub records_read: RecordCount,
    pub workouts_written: WorkoutCount,
    /// How many workouts the retractions actually removed. Distinct from
    /// `retractions_read`, which is how many withdrawal events were served: one
    /// naming a record that was never landed removes nothing.
    pub workouts_retracted: WorkoutCount,
    pub retractions_read: RecordCount,
    pub records_refused: RecordCount,
    pub refusals_recorded: RefusalCount,
}

/// What the last derivation would not accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusalReport {
    /// When the derivation that produced these ran, so a stale list reads as
    /// stale rather than as current.
    pub derived_at: Option<FetchedAt>,
    /// In the order they were produced: record order, then position within a
    /// record. Grouping by kind is the reader's job, not the store's.
    pub refusals: Vec<Refusal>,
}

// --- Driving ports ----------------------------------------------------------

/// Derives the normalised layer for a stream.
///
/// A driving port rather than something `cli` owns: `cli` implements nothing,
/// it constructs the use case and calls this, and a `web` handler will do the
/// same against the same signature. A capability only one transport can invoke
/// has been built into that transport.
pub trait WorkoutNormaliser {
    /// # Errors
    ///
    /// [`NormalisationError`] if the store is unavailable or the vocabulary
    /// has a gap. Never for a record the domain refused.
    fn normalise(
        &self,
    ) -> impl Future<Output = Result<NormalisationSummary, NormalisationError>> + Send;
}

/// Where a stream's derivation stands.
///
/// `records_behind` is what makes a forgotten `normalise` visible rather than
/// silent: raw's record count minus what the last successful derivation read.
/// Non-zero means raw has moved since and the normalised layer is stale, which
/// is § 38 applied to a derivation rather than only to an extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationStatus {
    pub last_success: Option<NormalisationRun>,
    pub workouts_held: WorkoutCount,
    pub refusals_held: RefusalCount,
    pub records_behind: RecordCount,
}

/// Reports where a stream's derivation stands.
pub trait DerivationStatusReporter {
    /// # Errors
    ///
    /// [`NormalisationError`] if the store is unavailable.
    fn derivation_status(
        &self,
    ) -> impl Future<Output = Result<DerivationStatus, NormalisationError>> + Send;
}

/// Reports what the domain would not accept.
pub trait RefusalReporter {
    /// # Errors
    ///
    /// [`NormalisationError`] if the store is unavailable.
    fn refusals(&self) -> impl Future<Output = Result<RefusalReport, NormalisationError>> + Send;
}

// ---------------------------------------------------------------------------
// Prescription (003)
//
// The one place § 11's permitted direction is exercised. A prescription may be
// derived by reading the performed record; nothing reads the other way, and no
// port below returns both kinds of value.
// ---------------------------------------------------------------------------

/// One working performance of one exercise, on one date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Performance {
    pub on: Date,
    /// Which landing record it came from. Kept so a prescription can be traced
    /// back to the observation it was derived from.
    pub landed_as: LandingRecordId,
    /// The prescribed session this performance was against, where the record
    /// names one.
    ///
    /// **The published id is the only thing that links the two.** A performance
    /// carries the reference the destination assigned when the session was
    /// delivered, and that reference resolves to the prescription it came from.
    /// Nothing else does: not the date, not the exercise, not the order they
    /// appear in.
    ///
    /// `None` is a session performed against no prescription of ours -- logged
    /// freehand, or against a routine that was never delivered from here.
    pub fulfilled: Option<FulfilledSession>,
    pub sets: Vec<PerformedSetSummary>,
}

/// The prescription a performance was performed against.
///
/// Two facts, because two is what reading a performance's role needs: whose
/// session it was, and which of that programme's sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FulfilledSession {
    /// The programme that prescribed it, **by name**.
    ///
    /// The name is a programme's identity across re-authorings and the row id
    /// is not: re-authoring writes a new `programme` row, so a `ProgrammeId`
    /// held here would stop matching every session prescribed before the last
    /// correction. `latest_of_each` picks by name for the same reason.
    pub programme: ProgrammeName,
    pub role: SessionRole,
}

/// Enough of a set for double progression and for the gate.
///
/// Deliberately not the whole `Set<M>`: rest is never recorded by the one source
/// in use, and the set kind is already filtered to working sets before this is
/// built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PerformedSetSummary {
    pub load: Load,
    /// A completed count, or a failed attempt. The gate reads this and nothing
    /// else; `intensity` is deliberately absent, because an effort report is not
    /// an input to any derivation.
    pub outcome: Performed<RepCount>,
}

/// What the projection knows about an exercise.
///
/// **Not `Option`.** Three states matter and they are different: performed
/// before, never performed, and performed but unusable. An `Option` collapses
/// the first two at the call site, and that is exactly the shape that invites a
/// `None` to become a default load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LastPerformance {
    Performed(Performance),
    NeverPerformed,
}

/// A slot the generator could not derive, and why.
///
/// A value rather than an error: FR-011 wants the system to say which slot and
/// why without substituting a guess, and the rest of the workout is still worth
/// issuing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnderivableSlot {
    pub slot: SlotId,
    pub exercise: &'static str,
    pub reason: UnderivableReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UnderivableReason {
    #[error("never performed, and the programme sets no starting load")]
    NeverPerformed,
    #[error("its last performance recorded no working set to progress from")]
    NoWorkingSet,
    /// A mobility slot filled with something that is not counted in time.
    #[error("a hold is counted in time, and this exercise is not")]
    NotAHold,
    /// The primary slot's fill is not counted in repetitions.
    #[error("a top set is a number of repetitions, and this exercise is not counted in them")]
    NotCountedInReps,
    /// Derivable in itself, but grouped with a slot that was not.
    #[error("the item it is supersetted with could not be derived")]
    GroupWithheld,
    /// The programme's span and duration do not make a ladder.
    #[error("the programme's span and duration do not make a ladder")]
    NoLadder,
    /// No scale has been authored for the implement this exercise is loaded on.
    ///
    /// Reported rather than defaulted to the barbell's steps: a prescription
    /// derived from an invented grid looks exactly like one derived from the
    /// gym's real equipment.
    #[error("no load scale has been authored for the implement this is loaded on")]
    NoLoadScale,
    /// A test whose target is inherited, with no programme before it to inherit
    /// from — or one whose predecessor trained a different lift, whose maximum
    /// says nothing about this one (decision 0013).
    #[error(
        "this test takes its target from the programme before it, and there is          none to take it from"
    )]
    NoTarget,
    /// The light session of a test week runs the predecessor's session, and
    /// there is no predecessor whose progression could say at what load.
    #[error(
        "the other session of a test week is the previous programme's, and          there is no previous programme"
    )]
    NoPredecessor,
    /// A block's entry-test week states no load for its other session, which is
    /// how the operator says they do not run it.
    #[error(
        "this block's entry-test week states no load for its other session, so \
         it runs only the test"
    )]
    NoEntryTestLightLoad,
}

/// The projection of the performed record that prescription reads.
///
/// **Typed to the repetitions vocabulary.** Double progression is a rule about
/// repetitions, and the outcome it reads is a `Performed<RepCount>` — so a
/// duration or distance exercise has no business reaching it. Narrowing the
/// signature is what makes that structural: asking for a hold's history is a
/// compile error rather than a row the adapter cannot decode.
pub trait ExerciseHistory {
    /// The most recent working performance of each exercise asked about.
    ///
    /// Unbounded in time: an alternating slot's exercise was last performed two
    /// sessions ago, not one. Batched rather than one call per slot, because
    /// eleven round trips to answer one question is the shape that becomes an
    /// N+1 the first time a programme grows.
    ///
    /// Where two landing records share a source record id, the later-served one
    /// is read (§ 10). Warm-ups are excluded.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn last_performances(
        &self,
        exercises: &[RepsExercise],
    ) -> impl Future<Output = Result<BTreeMap<RepsExercise, LastPerformance>, StoreError>> + Send;

    /// Every working performance of one exercise, oldest first.
    ///
    /// The ladder position needs this and `last_performances` cannot supply it:
    /// deciding whether the ladder advances, holds or suspends means asking of
    /// each gating session in turn whether its top set completed or failed, and
    /// whether a failed load had already been failed once. That is a series, not
    /// a latest value.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn performances(
        &self,
        exercise: RepsExercise,
    ) -> impl Future<Output = Result<Vec<Performance>, StoreError>> + Send;

    /// The newest performance in the record, whatever exercise it was of.
    ///
    /// § 38: a prescription derived from history that stops four days before the
    /// last session is visibly stale rather than quietly wrong.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn newest_performance(&self) -> impl Future<Output = Result<Option<Date>, StoreError>> + Send;
}

/// Whole performed workouts, for projecting into a prescription shape.
///
/// Separate from [`ExerciseHistory`] because it answers a different question at
/// a different grain, and merging them would give one port two reasons to
/// change. It returns the domain entity untouched, because projection operates
/// on the workout entire — its items, its groupings, its ordering.
pub trait PerformedWorkoutReader {
    /// Oldest first, § 10 applied.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn between(
        &self,
        from: Date,
        to: Date,
    ) -> impl Future<Output = Result<Vec<GymWorkout>, StoreError>> + Send;

    /// The workout performed against a prescription, and the reference it named.
    ///
    /// **Keyed on the prescription rather than on a date**, which is the whole
    /// point: a session prescribed for Friday and performed on Saturday morning
    /// is found by this and not by [`Self::between`]. The link is the published
    /// id — the prescription was delivered, the destination named it, and the
    /// performance carries that name — so this resolves through the delivery
    /// rather than asking which destination to look in. Any of them will do; a
    /// reference a performance names is a reference that was delivered.
    ///
    /// `None` where the prescription is drafted, or published and not yet
    /// performed. Both are ordinary states (§ 12.1), not faults.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn fulfilling(
        &self,
        prescription: PrescribedWorkoutId,
    ) -> impl Future<Output = Result<Option<(DeliveryReference, GymWorkout)>, StoreError>> + Send;
}

/// The § 14 parameters, in force as one version.
pub trait GenerationParameterStore {
    /// The greatest `authored_at`, with the version it came from.
    ///
    /// § 14 requires only the current value. Superseded rows are retained and no
    /// derivation reads one; an issued prescription names the version it used,
    /// which is what makes that safe.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn current(
        &self,
    ) -> impl Future<Output = Result<Option<(Timestamp, GenerationParameters)>, StoreError>> + Send;

    /// Author a set, superseding by date rather than overwriting (§ 12).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn author(
        &self,
        authored_at: Timestamp,
        parameters: &GenerationParameters,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
}

/// The authored programme.
pub trait ProgrammeStore {
    /// The programme that answers for a date, with the identity the store gave
    /// it.
    ///
    /// **By date rather than by recency** (decision 0012). Programmes succeed
    /// one another, so "the latest authored" is the wrong question: it would
    /// answer a September date from the block authored for October. Two
    /// programmes never cover one day, so at most one can match.
    ///
    /// `None` is a date no programme covers — between two blocks, or before the
    /// first. A real state, not a fault.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn on(
        &self,
        date: Date,
    ) -> impl Future<Output = Result<Option<(ProgrammeId, Programme)>, StoreError>> + Send;

    /// The programme immediately before a date, if there is one.
    ///
    /// **What a standalone test inherits from** (decision 0013). A test's target
    /// is where the predecessor's progression stands, and its light session is
    /// the predecessor's session — so deriving a test week needs the programme
    /// that finished before it, which [`Self::on`] by definition does not
    /// return.
    ///
    /// The latest one to have *finished* by the date, so a programme still
    /// running is not it. `None` is a test with nothing before it, which is why
    /// [`TestTarget::Declared`](domain::prescription::TestTarget::Declared)
    /// exists.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn preceding(
        &self,
        date: Date,
    ) -> impl Future<Output = Result<Option<(ProgrammeId, Programme)>, StoreError>> + Send;

    /// Every programme's name and the days it occupies, oldest first.
    ///
    /// What the overlap rule reads. It is here rather than inside [`Self::author`]
    /// because refusing an overlapping programme is a rule about authored data,
    /// and the core owns it — the store only reports what it holds.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn windows(&self) -> impl Future<Output = Result<Vec<ProgrammeWindow>, StoreError>> + Send;

    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn author(
        &self,
        programme: &Programme,
    ) -> impl Future<Output = Result<ProgrammeId, StoreError>> + Send;
}

/// What was issued.
pub trait PrescribedWorkoutStore {
    /// Record a prescription, in full. Written once and never rewritten (§ 12).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn issue(
        &self,
        workout: &PrescribedWorkout,
    ) -> impl Future<Output = Result<PrescribedWorkoutId, StoreError>> + Send;

    /// What is in force for a date, if anything.
    ///
    /// **A performed issue, if there is one; otherwise the newest.** A reissue
    /// supersedes rather than replaces (§ 12), so a date may hold several rows
    /// and exactly one of them is the answer — and once one has been trained,
    /// that is the one, whatever was derived afterwards. Decision 0021 states
    /// the rule; it is enforced here rather than in each caller, because
    /// `prescribe`, `deliver` and `compare` would otherwise each have to
    /// remember it and any of them could forget.
    ///
    /// Read before issuing, so asking twice for one date returns what was
    /// already issued rather than a second prescription. The derived ladder
    /// position makes double-advance structurally impossible; this makes the
    /// *output* idempotent too.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn issued_for(
        &self,
        date: Date,
    ) -> impl Future<Output = Result<Option<(PrescribedWorkoutId, PrescribedWorkout)>, StoreError>> + Send;
}

/// The identity the store gives an issued prescription.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrescribedWorkoutId(i64);

impl PrescribedWorkoutId {
    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for PrescribedWorkoutId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What asking for a date's prescription did.
///
/// **Four outcomes, not a flag.** `prescribe` used to answer "is this new?",
/// which was enough while asking twice could only ever return what was already
/// there. Now that the ordinary run derives again, the interesting question is
/// what the derivation *found* — and "unchanged" and "superseded" are the two
/// answers a boolean would collapse into one, while the very session the
/// operator is about to train from is the thing that differs between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Issuance {
    /// Nothing was in force for the date. Derived and stored.
    Issued,
    /// Derived, and it produced the session already in force. Nothing was
    /// written: an identical derivation is the same workout, and a second row
    /// saying so would be a second prescription to deliver.
    Unchanged,
    /// Derived, it differed, and this supersedes what was in force. The
    /// superseded prescription is kept (§ 12) and stops being the one in force.
    Superseded {
        previous: PrescribedWorkoutId,
        /// Set when the superseded prescription had been delivered — so the
        /// session at that destination is now stale, and nothing here can
        /// withdraw it.
        stranded: Option<DeliveryReference>,
    },
    /// A performance names the session in force, so it stands and no derivation
    /// was attempted. § 12.1: what it records happened.
    Performed { reference: DeliveryReference },
}

impl Issuance {
    /// Whether what is being reported was written by this call.
    pub const fn is_fresh(&self) -> bool {
        matches!(self, Self::Issued | Self::Superseded { .. })
    }
}

/// A prescription, and enough to report it honestly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prescription {
    pub id: PrescribedWorkoutId,
    pub workout: PrescribedWorkout,
    /// What asking did: issued, unchanged, superseded, or standing because it
    /// has been performed.
    pub issuance: Issuance,
    /// § 38. The newest performance the derivation read.
    pub history_through: Option<Date>,
    /// Slots that could not be derived (FR-011). Not an error.
    pub underivable: Vec<UnderivableSlot>,
}

/// Where a block's plan stands, for reporting rather than for prescribing.
///
/// **The whole of what a report needs and nothing a session needs.** § 38 asks
/// that staleness be observable, and on the prescribed side that means three
/// things at once: which programme is in force, where the ladder has got to, and
/// how current the record it was derived from is. A caller printing a ladder table
/// needs all three and needs no workout at all.
#[derive(Debug, Clone)]
pub struct LadderStanding {
    pub programme_id: ProgrammeId,
    pub programme: Programme,
    pub parameters: GenerationParameters,
    /// Derived from the gating sessions before the date asked about.
    /// Where the record puts the programme, for the one template that has a
    /// position to be at. `None` for a block, whose loads are shares of its
    /// anchor, and for a test, which has no ladder at all.
    pub progress: Option<Progress>,
    /// What a standalone test in force is an attempt at, as the record stands.
    ///
    /// Reported rather than stored, because it moves: every rung the predecessor
    /// makes raises it (decision 0011), so the number here is true of the moment
    /// it was asked for and of nothing else. `None` for any programme that is not
    /// a test, and for a test whose predecessor cannot supply one.
    pub target: Option<domain::gym::Kg>,
    /// The newest performance the derivation could see. `None` for an empty
    /// record — which is not the same as a stale one.
    pub history_through: Option<Date>,
}

/// Issue the prescription for a date.
pub trait WorkoutPrescriber {
    /// Where the plan stands, without issuing anything.
    ///
    /// Separate from [`Self::prescribe`] because it writes nothing: asking where
    /// the ladder is should not put a prescription in the store, and a report that
    /// issued one would change the thing it was reporting on.
    ///
    /// # Errors
    ///
    /// [`PrescriptionError`] for no programme, no parameters, or an unavailable
    /// store. Not for a date the programme does not run: a standing is about the
    /// block rather than about a session.
    fn standing(
        &self,
        on: Date,
    ) -> impl Future<Output = Result<LadderStanding, PrescriptionError>> + Send;

    /// The date. Nothing else, because there is nothing else to decide.
    ///
    /// The session role, the week and the ladder position are all derived — the
    /// role from the programme's calendar, the position from the performed
    /// record. Passing any of them would be passing a derived value, which is
    /// the mistake `HevyWorkoutLandingStore::STREAM` exists to avoid on the
    /// extraction side, and would let a caller prescribe a heavy session on a
    /// light day.
    ///
    /// **Whether to derive again is derived too**, which is why the `Reissue`
    /// argument that used to sit here is gone. It asked the caller a question
    /// the record already answers: a session that has been performed stands,
    /// and every other session is worth deriving because deriving it is how we
    /// find out whether it changed. What the caller could not have known — and
    /// was therefore being asked to guess — is whether the record has moved
    /// since the session was issued.
    ///
    /// Asking twice is still a question rather than an instruction, and still
    /// answers with one prescription: an identical derivation writes nothing.
    /// See [`Issuance`].
    ///
    /// # Errors
    ///
    /// [`PrescriptionError`] for no programme, no parameters, a date the
    /// programme does not run, or an unavailable store.
    fn prescribe(
        &self,
        date: Date,
    ) -> impl Future<Output = Result<Prescription, PrescriptionError>> + Send;
}

/// Where a prescription has got to: drafted, published, or performed.
///
/// **Derived from the relations, not read from a column.** A prescription with
/// no delivery is drafted; one whose delivery reference no workout names is
/// published; one a workout names is performed. Asking the store rather than
/// storing the answer is what stops the two disagreeing.
pub trait PrescriptionLifecycle {
    /// The state of one issued prescription.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something
    /// unreadable.
    fn state_of(
        &self,
        prescription: PrescribedWorkoutId,
    ) -> impl Future<Output = Result<PrescriptionState, StoreError>> + Send;
}

/// Store an authored programme and its parameters.
/// Which of the two things authoring did (decision 0012).
///
/// Reported rather than inferred by the operator from a changed row count: a
/// typo in the name would otherwise create a phantom programme silently, and
/// the difference between correcting a block and starting one is exactly what
/// they need to see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authored {
    /// The name was not in the store. A new programme.
    Created,
    /// The name was, so this supersedes that programme's previous version.
    Modified,
}

pub trait ProgrammeAuthor {
    /// Takes `domain` types.
    ///
    /// The document format is converted in `infrastructure`, so nothing here
    /// knows one exists — which is what keeps § 21's exemption honest.
    ///
    /// # Errors
    ///
    /// [`PrescriptionError`] if the store is unavailable, or the programme is
    /// inconsistent in a way the types could not catch.
    fn author(
        &self,
        programme: &Programme,
        parameters: &GenerationParameters,
    ) -> impl Future<Output = Result<(ProgrammeId, Authored), PrescriptionError>> + Send;
}

// --- Delivery ---------------------------------------------------------------
//
// **A destination is a renderer that returns a receipt.** Printing a session to
// a terminal and putting it in the app the operator trains from are the same
// act; the only asymmetry is that the second keeps what it was given, under an
// identity of its own. That identity is the whole of what crosses back over the
// port, and everything else about how a session is rendered — titles, folders,
// which of the source's templates an exercise is written to, what its notes say
// — is the adapter's and appears nowhere here.

// Re-exported so that every ring above reaches them through the port surface,
// as it does the rest of the vocabulary a port speaks.
pub use domain::prescription::{DeliveryReference, DestinationName, SessionOrdinal};

/// Everything a rendering needs that the prescription does not itself carry.
///
/// The prescription knows its date, role and week; it does not know its
/// programme's *name* or which session of that programme it is, because both are
/// facts about the calendar rather than about what was issued. Deriving them
/// here and handing them over keeps the destination from reading a store.
#[derive(Debug, Clone)]
pub struct Deliverable {
    pub workout: PrescribedWorkout,
    pub programme: ProgrammeName,
    /// Which session of the programme this is. What an ordered list of them is
    /// ordered by; how it is rendered is the destination's business.
    pub ordinal: SessionOrdinal,
}

/// Something the destination has no way to state.
///
/// A value rather than an error, for the reason [`UnderivableSlot`] is one: the
/// rest of the session is still worth delivering, and the operator needs to know
/// which part of it did not arrive. What is *not* here is anything the
/// destination renders differently rather than loses — an effort target that
/// becomes a note is expressed, not omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unexpressed {
    pub exercise: domain::gym::Exercise,
    pub reason: String,
}

/// What a destination did with a session.
#[derive(Debug, Clone)]
pub struct Delivered {
    /// What the destination called it. Opaque: see
    /// [`domain::prescription::delivery`].
    pub reference: DeliveryReference,
    pub unexpressed: Vec<Unexpressed>,
}

/// Where a prescription goes to be acted on.
///
/// The mirror of [`WorkoutEventSource`]: that one is where observations come
/// from, this is where intentions go. Named for the thing rather than the act,
/// so the adapter is a destination and `deliver` is what you do to it.
pub trait PrescriptionDestination {
    /// What this destination is called, bound at construction. The use case
    /// reads it from here rather than taking it as an argument, so a delivery
    /// cannot be recorded against a destination it did not go to.
    fn name(&self) -> &DestinationName;

    fn deliver(
        &self,
        session: &Deliverable,
    ) -> impl Future<Output = Result<Delivered, DeliveryError>> + Send;
}

/// **Borrowed destinations are destinations.** A caller that needs to keep hold
/// of its destination — to ask a preview what it rendered — should not have to
/// choose between that and handing it to the use case.
impl<T: PrescriptionDestination + Sync> PrescriptionDestination for &T {
    fn name(&self) -> &DestinationName {
        (*self).name()
    }

    fn deliver(
        &self,
        session: &Deliverable,
    ) -> impl Future<Output = Result<Delivered, DeliveryError>> + Send {
        (*self).deliver(session)
    }
}

/// What has already been delivered, and where.
///
/// One table across destinations rather than one per destination — the opposite
/// of [`LandingStore`], and for a reason that is not an inconsistency: a landing
/// table is *shaped* by its stream, and a delivery record is the same three
/// columns whatever received it. So the destination is a key here rather than a
/// binding, and the use case supplies it from the destination it holds.
pub trait PrescriptionDeliveryStore {
    /// What this prescription was delivered as, if it has been.
    ///
    /// Read before delivering, so asking twice produces one routine rather than
    /// two. A reissue is a different prescription and so has no record here,
    /// which is what makes "one delivery per issued prescription" fall out of
    /// § 12 rather than out of anything the destination imposes.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn reference_for(
        &self,
        prescription: PrescribedWorkoutId,
        destination: &DestinationName,
    ) -> impl Future<Output = Result<Option<DeliveryReference>, StoreError>> + Send;

    /// Record a delivery. Written once and never rewritten (§ 12).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn record(
        &self,
        prescription: PrescribedWorkoutId,
        destination: &DestinationName,
        reference: &DeliveryReference,
        at: Timestamp,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
}

/// What a delivery amounted to.
#[derive(Debug, Clone)]
pub struct Delivery {
    pub reference: DeliveryReference,
    pub destination: DestinationName,
    pub ordinal: SessionOrdinal,
    /// False when the prescription had already been delivered and this is that
    /// delivery. The output side of the same idempotence
    /// [`PrescribedWorkoutStore::issued_for`] gives issuing.
    pub freshly_delivered: bool,
    /// Empty on a session the destination could state in full.
    pub unexpressed: Vec<Unexpressed>,
}

/// Put the prescription for a date where the operator trains from.
pub trait PrescriptionDeliverer {
    /// Deliver what was issued for `date`.
    ///
    /// Takes no prescription: what is in force for a date is derived, exactly as
    /// it is for [`WorkoutPrescriber::prescribe`], and passing one would let a
    /// caller deliver a superseded session.
    ///
    /// # Errors
    ///
    /// [`DeliveryError`] if nothing is issued for the date, the destination is
    /// unreachable, or the store is unavailable.
    fn deliver(&self, date: Date) -> impl Future<Output = Result<Delivery, DeliveryError>> + Send;
}

// --- The operator's week ----------------------------------------------------
//
// **Operator-level, not programme-level.** A schedule is a fact about a life,
// and every discipline reads it while none owns it. Nothing here allocates a
// slot to anything: which of the operator's evenings the gym may use is
// planning, and planning waits.

/// Everything the operator has said about their week.
pub trait DiaryStore {
    /// The whole diary: every ordinary pattern, and every alteration that
    /// departs from one.
    ///
    /// **Whole rather than by date**, unlike [`ProgrammeStore::on`]. `Diary`
    /// owns the rule that resolves a date — the week in force, as amended by
    /// any alteration covering it — and answering a date here would put that rule in
    /// a second place, where the two could disagree.
    ///
    /// An empty diary is a real state: a machine on which nobody has said
    /// anything about their week yet.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something
    /// unreadable.
    fn diary(&self) -> impl Future<Output = Result<Diary, StoreError>> + Send;
}

/// Record what the operator has said about their week.
///
/// Split from [`DiaryStore`] for the reason the landing ports are split: a
/// reader is what almost everything needs, and a capability nothing but
/// authoring uses should not be reachable from everything that reads.
pub trait DiaryAuthor {
    /// Record an ordinary pattern, in force from its own date.
    ///
    /// Not an update: a pattern is superseded by a later one existing, so this
    /// only ever adds. Re-stating a pattern already in force from that date
    /// replaces it, which is a correction rather than a succession.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn record_pattern(
        &self,
        schedule: &TrainingPattern,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;

    /// Record an alteration — a run of days that departs from the pattern.
    ///
    /// Keyed on the day it starts, so re-stating the one that begins on the
    /// 14th corrects it rather than recording a second.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn record_alteration(
        &self,
        alteration: &Alteration,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
}
