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

use domain::landing::{
    EventKind, EventTime, ExtractionRun, FetchedAt, LandingRecord, LandingStream, PayloadDigest,
    RawPayload, RecordCount, RunId, RunOutcome, SourceRecordId, Watermark,
};

use crate::{
    error::{ExtractionError, RunLockError, SourceError, StatusError, StoreError},
    paging::{PageCount, PageNumber},
};

// --- Driven ports -----------------------------------------------------------

/// One event, already split out of its page and carrying its bytes intact.
///
/// The split happens in the adapter because only the adapter knows the page's
/// shape; what crosses the port is one record as served, which is the unit a
/// landing record corresponds to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceEvent {
    pub kind: EventKind,
    pub source_record_id: SourceRecordId,
    pub event_time: Option<EventTime>,
    pub payload: RawPayload,
}

/// One page of events, in the order the source served them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventPage {
    pub page: PageNumber,
    pub page_count: PageCount,
    pub events: Vec<SourceEvent>,
}

/// Where observations come from.
pub trait WorkoutEventSource {
    /// One page of events, newest first.
    ///
    /// `since` is inclusive at the source, so a caller passes its stored
    /// watermark unmodified: the boundary event is served again and
    /// deduplicated, which costs nothing and cannot skip a sibling sharing
    /// that timestamp.
    ///
    /// # Errors
    ///
    /// [`SourceError`] if the source is unreachable, rejects our credential,
    /// or serves something unreadable.
    fn fetch_page(
        &self,
        since: Option<Watermark>,
        page: PageNumber,
    ) -> impl Future<Output = Result<EventPage, SourceError>> + Send;
}

/// Raw landing for **one** stream.
///
/// Each landing table has its own instance, bound at construction, so there is
/// no stream argument to pass wrongly and a store for `hevy.workouts` cannot
/// read another stream's records.
pub trait LandingStore {
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
    type Guard;

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

/// What a completed run is worth reporting.
///
/// `events_seen` and `records_landed` are both present and both reported: the
/// difference between them is what distinguishes a run that found nothing new
/// from one that found nothing at all, and neither from a failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSummary {
    pub run_id: RunId,
    pub events_seen: domain::landing::EventCount,
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

/// Collect everything the source has served since the resumption point.
pub trait ExtractWorkouts {
    /// # Errors
    ///
    /// [`ExtractionError::AlreadyRunning`] when another run holds the lock, in
    /// which case nothing is landed and the resumption point does not move.
    /// Otherwise the underlying source or store failure.
    fn extract(
        &self,
        stream: &LandingStream,
    ) -> impl Future<Output = Result<RunSummary, ExtractionError>> + Send;
}

/// Report the most recent successful extraction, so a broken one is visible.
pub trait ReportExtractionStatus {
    /// # Errors
    ///
    /// [`StatusError`] if the store is unavailable. Never having run is not an
    /// error — it is a fact to report.
    fn status(
        &self,
        stream: &LandingStream,
    ) -> impl Future<Output = Result<StreamStatus, StatusError>> + Send;
}

/// Discard a stream's resumption point so the next run collects everything.
pub trait ResetResumptionPoint {
    /// Lands nothing and removes nothing.
    ///
    /// # Errors
    ///
    /// [`StatusError`] if the store is unavailable.
    fn reset(&self, stream: &LandingStream)
    -> impl Future<Output = Result<(), StatusError>> + Send;
}
