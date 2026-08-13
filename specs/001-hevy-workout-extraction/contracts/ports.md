# Contract: application ports

The interfaces `application` declares and the other rings implement. Ports are
declared in the application's own vocabulary (§ 16); no `reqwest`, `sqlx`,
`serde` or `jiff` type appears in a signature, and no vendor error crosses
inward (§ 26).

Signatures are indicative — they fix the boundary, not the implementation.

Traits are named for the thing that implements them rather than for the act:
`WorkoutExtractor`, not `ExtractWorkouts`. The act is the method.

## Driving ports

Invoked by `cli`. Each corresponds to one command in [cli.md](./cli.md).

```rust
pub trait WorkoutExtractor {
    /// Collect everything the source has served since the resumption point.
    ///
    /// # Errors
    ///
    /// [`ExtractionError::AlreadyRunning`] when another run holds the lock
    /// (FR-010); no records are landed and the resumption point does not move.
    fn extract(&self) -> impl Future<Output = Result<RunSummary, ExtractionError>> + Send;
}

pub trait ExtractionStatusReporter {
    /// The most recent successful extraction for one stream (FR-008, § 38).
    fn status(&self) -> impl Future<Output = Result<StreamStatus, StatusError>> + Send;
}

pub trait ResumptionPointResetter {
    /// Discard the resumption point so the next run collects the full history
    /// (FR-007). Lands nothing and removes nothing.
    fn reset(&self) -> impl Future<Output = Result<(), StatusError>> + Send;
}
```

### Which stream, and why none of them asks

None of the three takes a stream, and neither does the constructor of either
use case. Each is built from ports already bound to one and reads it back out
of them — see `LandingStore::stream` below.

A stream supplied alongside the ports would be a second answer to a question
that already has one, and two answers can disagree. The failure is not
cosmetic: a run whose lock, run log and resumption point name one stream while
its records are tagged with another takes the wrong lock (so two real runs of
the same stream proceed at once, defeating FR-010), and on success advances a
watermark belonging to a stream it never collected — which makes the *next*
run of that stream skip everything before that point, silently and permanently.
D1's invariant is that the point never passes an event the run observed; here
it would pass events never asked for.

So the identity of a run comes from the adapters doing the work, and there is
nowhere to pass a different one.

`RunSummary` carries `events_seen` and `records_landed` separately — FR-011
depends on the difference being visible, not inferred.

## Driven ports

Implemented in `infrastructure`.

```rust
pub trait WorkoutEventSource {
    /// Whatever this adapter needs to continue a walk it has started —
    /// a page number, a cursor, an offset into a file. Opaque here: how a
    /// source instalments its answer is a fact about the source, not about
    /// the data, and a run has no use for a number it cannot land.
    type Resume: Send;

    /// The next instalment of everything served since `since`.
    ///
    /// `since` is inclusive at the source, so the caller may pass the stored
    /// watermark unmodified (research.md). It is passed on every call rather
    /// than remembered, so the adapter holds no state between them.
    fn fetch(&self, since: Option<Watermark>, resume: Option<Self::Resume>)
        -> impl Future<Output = Result<EventBatch<Self::Resume>, SourceError>> + Send;
}

/// `resume` is `None` when the source has served everything.
pub struct EventBatch<R> {
    pub events: Vec<SourceEvent>,
    pub resume: Option<R>,
}

/// One record as served, already split out of its batch (FR-001) with its
/// bytes intact (FR-002). The adapter supplies the provenance, because the
/// adapter is the only thing that knows how it was asked.
pub struct SourceEvent {
    pub source_record_id: SourceRecordId,
    pub provenance: Provenance,
    pub payload: RawPayload,
}

/// Raw landing for **one** stream. Each landing table has its own instance,
/// bound at construction — so there is no stream parameter to pass wrongly, and
/// a store for `hevy.workouts` cannot read `hevy.routines`.
pub trait LandingStore {
    /// Which stream this store holds.
    ///
    /// Asked rather than told. Being bound to one table makes this the only
    /// port that can answer without being informed, so it is the single source
    /// of truth for a run's identity.
    fn stream(&self) -> &LandingStream;

    /// Digest of the most recent landing record for this source record, if
    /// any — the comparison FR-005 and scenario 6 specify.
    fn latest_digest(&self, id: &SourceRecordId)
        -> impl Future<Output = Result<Option<PayloadDigest>, StoreError>> + Send;

    /// Append records. Never updates, never deletes (§ II.1).
    fn append(&self, run: RunId, records: Vec<LandingRecord>)
        -> impl Future<Output = Result<RecordCount, StoreError>> + Send;

    /// How many records this stream holds in total. Reported by `status`.
    fn count(&self) -> impl Future<Output = Result<RecordCount, StoreError>> + Send;
}

pub trait ResumptionPointStore {
    fn read(&self, stream: &LandingStream)
        -> impl Future<Output = Result<Option<Watermark>, StoreError>> + Send;

    fn advance(&self, stream: &LandingStream, to: Watermark, at: FetchedAt)
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
    /// `Send`: a run holds the guard across every await it makes.
    type Guard: Send;
    fn try_acquire(&self, stream: &LandingStream) -> Result<Self::Guard, RunLockError>;
}

pub trait Clock {
    fn now(&self) -> FetchedAt;
}
```

`Clock` is a port so that `fetched_at` is injectable and run behaviour is
testable without sleeping. It is *not* how the watermark is set — D1 forbids
that.

### What is not here

Pagination. `PageNumber` and `PageCount` live in the Hevy adapter, because a
page is an artefact of how that source answers: page boundaries are preserved
nowhere, a landing record corresponds to one workout as served, and a CSV
export has no pages at all. The port says "here is a batch, and here is what to
ask for next, if anything" — which a paginated API, a cursor API and a single
file can all answer.

### Reaching past a port

The chain that identity travels is: the table named in the adapter's SQL, to
that adapter's declared stream, to the run's lock, run log, resumption point
and record tags. Every link but the first is derived rather than passed, so the
first is the only one a person can get wrong — and it is one constant sitting
beside the queries that name the table, pinned by a test in `infrastructure`
and tied to the catalogue by a test in `cli`.

`infrastructure` depends on `application` because that is where the ports it
implements are declared. It may name those ports and their errors, and nothing
else: an adapter calling a use case would make the driven side drive. The use
cases stay behind `application::extract` and `application::status`, and the
`use-case-isolation` flake check enforces it.

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
