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

use std::future::Future;

use domain::gym::{
    GymWorkout, NonEmpty, NormalisationOutcome, NormalisationRun, NormalisationRunId, OperatorZone,
    Refusal, RefusalCount, WorkoutCount,
};
use domain::landing::{
    EventCount, ExtractionRun, FetchedAt, LandedRecord, LandingRecord, LandingStream, PayloadDigest,
    Provenance, RawPayload, RecordCount, RunId, RunOutcome, SourceRecordId, Watermark,
};

use crate::error::{
    ExtractionError, NormalisationError, RunLockError, SourceError, StatusError, StoreError,
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
/// The three outcomes of FR-005, as a sum. A record that produced no workout
/// and no reason does not compile, and a retraction cannot carry refusals —
/// the two mistakes most worth making impossible, since either would let a
/// record go silently missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Translation {
    /// A workout, with whatever it would not accept listed beside it. A
    /// refusal inside a record does not stop the rest of it translating
    /// (FR-024), so both travel together.
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

/// What a completed derivation is worth reporting.
///
/// Numbers that must add up: `records_read` equals `workouts_written` plus
/// `workouts_withdrawn` plus `retractions_applied` plus `records_refused`.
/// That is FR-005 asserted without reading a row, and it is printed at the
/// terminal for the same reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalisationSummary {
    pub run_id: NormalisationRunId,
    pub records_read: RecordCount,
    pub workouts_written: WorkoutCount,
    /// Workouts a retraction removed. Distinct from `retractions_applied`: a
    /// retraction naming a record never landed withdraws nothing.
    pub workouts_withdrawn: WorkoutCount,
    pub retractions_applied: RecordCount,
    pub records_refused: RecordCount,
    pub refusals_recorded: RefusalCount,
}

/// What the last derivation would not accept, grouped for reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefusalReport {
    pub stream: LandingStream,
    pub derived_at: Option<FetchedAt>,
    /// In the order they were produced: record order, then position within a
    /// record. Grouping by kind is the reader's job, not the store's.
    pub refusals: Vec<Refusal>,
}

// --- Driving ports ----------------------------------------------------------

/// Derives the normalised layer for a stream.
///
/// This trait existing at all is what satisfies FR-029: `cli` implements
/// nothing, it constructs the use case and calls this, and a future `web`
/// handler does the same against the same signature.
pub trait WorkoutNormaliser {
    /// # Errors
    ///
    /// [`NormalisationError`] if the store is unavailable or the vocabulary
    /// has a gap. Never for a record the domain refused.
    fn normalise(
        &self,
    ) -> impl Future<Output = Result<NormalisationSummary, NormalisationError>> + Send;
}

/// Reports what the domain would not accept.
pub trait RefusalReporter {
    /// # Errors
    ///
    /// [`NormalisationError`] if the store is unavailable.
    fn refusals(&self) -> impl Future<Output = Result<RefusalReport, NormalisationError>> + Send;
}
