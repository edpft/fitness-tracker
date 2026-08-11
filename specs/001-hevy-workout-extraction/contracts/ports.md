# Contract: application ports

The interfaces `application` declares and the other rings implement. Ports are
declared in the application's own vocabulary (§ 16); no `reqwest`, `sqlx`,
`serde` or `jiff` type appears in a signature, and no vendor error crosses
inward (§ 26).

Signatures are indicative — they fix the boundary, not the implementation.

## Driving ports

Invoked by `cli`. Each corresponds to one command in [cli.md](./cli.md).

```rust
pub trait ExtractWorkouts {
    /// Collect everything the source has served since the resumption point.
    ///
    /// # Errors
    ///
    /// [`ExtractionError::AlreadyRunning`] when another run holds the lock
    /// (FR-010); no records are landed and the resumption point does not move.
    fn extract(&self, stream: LandingStream)
        -> impl Future<Output = Result<RunSummary, ExtractionError>> + Send;
}

pub trait ReportExtractionStatus {
    /// The most recent successful extraction per source (FR-008, § 38).
    fn status(&self)
        -> impl Future<Output = Result<Vec<SourceStatus>, StatusError>> + Send;
}

pub trait ResetResumptionPoint {
    /// Discard the resumption point so the next run collects the full history
    /// (FR-007). Lands nothing and removes nothing.
    fn reset(&self, stream: LandingStream)
        -> impl Future<Output = Result<(), StatusError>> + Send;
}
```

`RunSummary` carries `events_seen` and `records_landed` separately — FR-011
depends on the difference being visible, not inferred.

## Driven ports

Implemented in `infrastructure`.

```rust
pub trait WorkoutEventSource {
    /// One page of events, newest first.
    ///
    /// `since` is inclusive at the source, so the caller may pass the stored
    /// watermark unmodified (research.md).
    fn fetch_page(&self, since: Option<Watermark>, page: PageNumber)
        -> impl Future<Output = Result<EventPage, SourceError>> + Send;
}

pub struct EventPage {
    pub page: PageNumber,
    pub page_count: PageCount,
    pub events: Vec<SourceEvent>,
}

/// One event, already split out of its page (FR-001) with its bytes intact
/// (FR-002).
pub struct SourceEvent {
    pub kind: EventKind,
    pub source_record_id: SourceRecordId,
    pub event_time: Option<EventTime>,
    pub payload: RawPayload,
}

/// Raw landing for **one** stream. Each landing table has its own instance,
/// bound at construction — so there is no stream parameter to pass wrongly, and
/// a store for `hevy.workouts` cannot read `hevy.routines`.
pub trait LandingStore {
    /// Digest of the most recent landing record for this source record, if
    /// any — the comparison FR-005 and scenario 6 specify.
    fn latest_digest(&self, id: &SourceRecordId)
        -> impl Future<Output = Result<Option<PayloadDigest>, StoreError>> + Send;

    /// Append records. Never updates, never deletes (§ II.1).
    fn append(&self, run: RunId, records: Vec<LandingRecord>)
        -> impl Future<Output = Result<RecordCount, StoreError>> + Send;
}

pub trait ResumptionPointStore {
    fn read(&self, stream: &LandingStream)
        -> impl Future<Output = Result<Option<Watermark>, StoreError>> + Send;

    fn advance(&self, stream: &LandingStream, to: Watermark)
        -> impl Future<Output = Result<(), StoreError>> + Send;

    fn clear(&self, stream: &LandingStream)
        -> impl Future<Output = Result<(), StoreError>> + Send;
}

pub trait ExtractionRunLog {
    fn begin(&self, stream: &LandingStream, at: FetchedAt)
        -> impl Future<Output = Result<RunId, StoreError>> + Send;

    fn finish(&self, run: RunId, outcome: RunOutcome)
        -> impl Future<Output = Result<(), StoreError>> + Send;

    fn latest_success(&self, stream: &LandingStream)
        -> impl Future<Output = Result<Option<ExtractionRun>, StoreError>> + Send;
}

/// FR-010. The guard releases on drop, and the kernel drops it if the process
/// dies (D7) — a crashed run leaves nothing to unstick.
pub trait RunLock {
    type Guard;
    fn try_acquire(&self, stream: &LandingStream) -> Result<Self::Guard, RunLockError>;
}

pub trait Clock {
    fn now(&self) -> FetchedAt;
}
```

`Clock` is a port so that `fetched_at` is injectable and run behaviour is
testable without sleeping. It is *not* how the watermark is set — D1 forbids
that.

### Why `impl Future + Send` rather than bare `async fn`

`async fn` in a trait produces a future with no `Send` bound, which cannot be
held across an `await` in a multi-threaded runtime or an axum handler. Writing
the bound explicitly costs one line per method now and avoids re-declaring every
port when the `web` ring acquires an HTTP surface.

## Error translation

Each port's error type is the application's view of a failure. Adapters
translate at the boundary; nothing below the line appears above it.

| Port | Application error | Adapter translates from |
| --- | --- | --- |
| `WorkoutEventSource` | `SourceError::{Unavailable, Unauthorised, Malformed, RateLimited}` | `reqwest::Error`, HTTP status, `serde_json::Error` |
| `LandingStore`, `ResumptionPointStore`, `ExtractionRunLog` | `StoreError::{Unavailable, Corrupt}` | `sqlx::Error`, SQLite result codes |
| `RunLock` | `RunLockError::{Held, Unavailable}` | `std::io::Error` |

`SourceError::Unavailable` is what makes scenario 7 and § 36 work: the source
being unreachable ends this run, leaves raw untouched, and does not implicate
any other capability.
