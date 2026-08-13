# Phase 1 Data Model: Hevy workout extraction

**Feature**: [spec.md](./spec.md) | **Research**: [research.md](./research.md)

Three entities, all from the spec: the landing record, the extraction run, and
the resumption point. No fitness domain entity appears here — no workout, no
exercise, no set. That is the whole point of the feature (§ II.1).

## Where these types live, and why

The value types below live in `crates/domain`, not in `application`.

The spec says this feature "produces no domain entities", and that is true of
the *fitness* model: nothing here knows what a set or a rep is. But a landing
record is not plumbing either — § II.1 makes raw landing the first **input** of
the observation data model, with rules of its own (append-only, provenance
mandatory, unrecognised fields retained). Those rules are exactly what § VI asks
the innermost ring to make unrepresentable-if-violated, and they name no adapter,
no transport and no store.

`application` declares the ports in these terms; `infrastructure` implements
them. A landing record therefore crosses two boundaries, and § 23 forbids it
being made of raw primitives at either.

---

## Landing record

One workout payload as the source served it, plus its provenance. Immutable.

FR-001 makes the unit one workout, not one page. FR-002 makes the payload
opaque. FR-003 fixes the provenance set.

The record carries what is true of every record whatever served it. What is
true only of the transport that carried it is in its `Provenance`, so that a
source arriving as a CSV export is a new variant rather than three columns left
blank.

| Field | Type | Notes |
| --- | --- | --- |
| `stream` | `LandingStream` | Which stream it belongs to. `hevy.workouts`. Carried by the type; *not* a column — the table name encodes it (see Store schema) |
| `fetched_at` | `FetchedAt` | When the fetch that produced it ran (FR-003) |
| `source_record_id` | `SourceRecordId` | The source's own identifier (FR-003, § 8) |
| `provenance` | `Provenance` | How it reached us (FR-003) — see below |
| `payload` | `RawPayload` | Exact bytes as received (FR-002) |
| `digest` | `PayloadDigest` | SHA-256 of `payload`, for change detection (D3) |

### Provenance

```rust
pub enum Provenance {
    /// Served by an HTTP feed of change events.
    Event(EventProvenance),
}
```

| Field of `EventProvenance` | Type | Notes |
| --- | --- | --- |
| `endpoint` | `Endpoint` | What was called. `/v1/workouts/events` |
| `kind` | `EventKind` | What the source said happened (FR-003) |
| `occurred_at` | `Option<EventTime>` | `updated_at` / `deleted_at`. Optional — § 37, D13 |

One variant, because one transport has been built. It is an enum rather than
more fields on the record so that the second transport is an addition instead
of a rewrite — a CSV export has no endpoint, no event kind and no event time,
and giving it empty ones would be recording facts we do not have. What a new
variant may not do is add to what *every* record carries.

`Provenance::occurred_at` is the one question askable without knowing what
carried the payload, because it is where a resumption point comes from. Every
other question requires matching the variant, which is how the store's HTTP
columns stay honest: a second variant fails to compile there.

### Value types

| Type | Wraps | Invariant enforced at construction (§ 24) |
| --- | --- | --- |
Every one of these is built by `TryFrom`, and gets `TryFrom<&str>`, `FromStr`,
`Display` and `AsRef` from that — no bespoke `new`, `parse` or `from_source`.
`RunId` is unsigned, and the store narrows SQLite's `i64` at the boundary.

| Type | Wraps | Invariant enforced at construction (§ 24) |
| --- | --- | --- |
| `SourceName` | `String` | Non-empty, lowercase, no whitespace, no `.` |
| `EntityKind` | `String` | The same rules, for the same reason |
| `LandingStream` | `SourceName` + `EntityKind` | One source's one entity type — `hevy.workouts`. The unit that resumes, runs and locks independently, and the name an operator types |
| `Endpoint` | `String` | Non-empty, no whitespace, begins `/` |
| `SourceRecordId` | `String` | Non-empty. Nothing else — see below |
| `FetchedAt` | `jiff::Timestamp` | — |
| `EventTime` | `jiff::Timestamp` | — |
| `RawPayload` | `Vec<u8>` | Non-empty |
| `PayloadDigest` | `[u8; 32]` | Fixed width. Ordinarily obtained by digesting a `RawPayload`; see below |
| `EventKind` | sum type | `Updated` \| `Deleted` \| `Unrecognised(RawEventKind)` |
| `RawEventKind` | `String` | Non-empty. Verbatim, never normalised (D12) |
| `RunId` | `u64` | A position in the store's sequence; negative is corruption |

**The name rules are ours, and they exist for one reason.** A stream is written
`hevy.workouts` on the command line, in a resumption point's key and in every
message an operator reads, so that text form has to round-trip. A `.` inside a
half would make the split ambiguous, whitespace would make the argument need
quoting, and mixed case would let one stream be spelled two ways and resume
from two different places. None of it is imposed on a value a source owns.

**`SourceRecordId` is not a UUID, and is barely constrained at all.** Hevy
serves UUIDs today and the OpenAPI document says so, but validating that shape
would be interpreting a source field, which FR-002 forbids, and would make
extraction fail on a source that changed its id format — losing data to defend
a constraint we do not own. The same argument rules out rejecting whitespace in
it. Being non-empty is the one requirement, and it is ours rather than the
source's: FR-003 has no meaning without an identifier.

**`EventKind::Unrecognised` is deliberate.** § 24 asks that illegal states be
unrepresentable; an event type Hevy adds next year is not illegal, it is
unknown. Modelling it as a variant is what lets § II.1's "unrecognised fields
are retained, never discarded" hold without the type lying about what it has
seen. `Updated` and `Deleted` are still distinguishable in the type system, so
no caller can confuse them.

**`PayloadDigest` guarantees width, and almost guarantees provenance.** The
ordinary way to obtain one is `RawPayload::digest`, so D3's comparison cannot be
fed a hand-made value. Two escape hatches exist for the persistence boundary
alone — `From<[u8; 32]>` and `TryFrom<&[u8]>` — because the store must rehydrate
a digest it wrote earlier without reading back the payload merely to re-derive
it, and a row of any other width means the file holds something this program did
not write. The type still makes a digest unconfusable with arbitrary data
everywhere.

**The digest is not `Hash`.** `Hash` hands its bytes to the caller's `Hasher`
and so cannot choose an algorithm, produces 64 bits, and guarantees nothing
across builds or platforms. This value is written to the store and compared
against rows written by earlier versions of this program, so all three matter.
`RawPayload` derives `Hash` as well, for putting a payload in a map — a
different job with different requirements.

### Immutability

There is no setter, no `&mut` accessor, and no update path through any port. The
store enforces it independently with `BEFORE UPDATE` / `BEFORE DELETE` triggers
(D6), so the guarantee does not rest on the type alone.

---

## Extraction run

One invocation: when it started, what it collected, whether it completed.
FR-008 and FR-011 are both about being able to tell these apart.

| Field | Type | Notes |
| --- | --- | --- |
| `id` | `RunId` | Assigned by the store |
| `stream` | `LandingStream` | `hevy.workouts` |
| `started_at` | `FetchedAt` | |
| `outcome` | `RunOutcome` | Sum type — see below |

```text
RunOutcome
├── InFlight
├── Succeeded { finished_at, events_seen: EventCount, records_landed: RecordCount }
└── Failed    { finished_at, reason: FailureReason }
```

A run cannot be both succeeded and failed, and cannot report landed records
without having finished — the sum type carries the counts in the `Succeeded`
variant rather than as optional fields beside it.

**`events_seen` and `records_landed` are both required, and differ.** This is
FR-011's mechanism. A successful run that saw 40 events and landed 0 was a real
run that found nothing new; a successful run that saw 0 events found nothing at
all; a failed run is neither. All three are distinguishable without inference.

`FailureReason` is a sum type, not a string: `SourceUnavailable`,
`Unauthorised`, `AlreadyRunning`, `MissingProvenance`, `StoreFailure` — each
translated at its port so no vendor error crosses inward (§ 26).

---

## Resumption point

The position extraction continues from. FR-007: reconstructible state, not a
system of record. § II governs it by explicitly declining to govern it.

| Field | Type | Notes |
| --- | --- | --- |
| `stream` | `LandingStream` | One per stream, not per source |
| `watermark` | `Watermark` | Newest event time seen by the last successful run (D1) |
| `updated_at` | `FetchedAt` | When it last moved |

`Watermark` wraps a `jiff::Timestamp`. Absence is meaningful and is modelled as
`Option<Watermark>` at the port: no row means "never run", which requests from
the epoch. Reset (FR-007) deletes the row.

### The rule that makes it safe

> The watermark advances to the newest event time the run **actually saw**, and
> never to the wall clock.

D1 has the reasoning. It is stated here because it is the single invariant that
FR-006 rests on, and because the failure it prevents is silent.

Corollaries:

- A run that saw no events leaves the watermark unmoved. With `since` inclusive
  and the feed ordered newest-first, "no events since the watermark" means
  nothing exists to advance to.
- A run that fails leaves it unmoved regardless of what it landed (FR-006).
- Because `since` is **inclusive** (verified — research.md), re-requesting from
  exactly the stored watermark re-serves the boundary event and skips nothing.
  The duplicate deduplicates by digest and lands nothing (FR-005).

---

## Relationships

```text
LandingStream ─┬─< LandingRecord     (append-only, many per workout over time)
 'hevy.workouts'├─< ExtractionRun     (many, ordered by started_at)
                └─── ResumptionPoint  (0..1)

LandingRecord >── ExtractionRun       (which run landed it)
```

A **landing stream** is one source's one entity type — `hevy.workouts`. It names
the unit that resumes, runs and locks independently, and it maps one-to-one onto
a landing table. Extracting Hevy workouts and Hevy body measurements are two
streams, with two watermarks, that never wait on each other.

A landing record records which run landed it. This is provenance about our own
collection rather than about the source, and it is what lets a run report
`records_landed` and lets an operator ask what a given run brought in. It does
not weaken FR-003, which is about the source.

Ordering within a run is recorded as a serve ordinal — the position in which the
source served the event. The spec's edge case requires payloads landed "in the
order served", and the feed serves newest-first, so the ordinal preserves the
source's ordering rather than imposing ours.

**The serve ordinal does not decide supersession.** § 10 resolves that at the
canonical layer from source-recorded event times. Two records for one workout
are the source contradicting itself, and which one is current is not a question
raw answers.

---

## Store schema

SQLite. Timestamps are RFC 3339 UTC `TEXT` (D10 — SQLite has no timestamp type,
and the conversion belongs at the adapter).

### One landing table per source *and* entity type

**Decision**: raw landing is **not** one shared table with a `source`
discriminator. Each (source, kind of thing landed) gets its own table. This
feature creates exactly one: `hevy_workout_landing`. Hevy routines, Hevy
exercise templates and Withings measurements would each get their own when they
arrive.

Consequences that shape everything below:

- **No `source` column.** The table name carries it, and it cannot vary within a
  table. FR-003's "what did this come from" is answered structurally rather than
  repeated 164 times.
- **`endpoint` stays.** It *can* vary within the table — a workout re-fetched
  from `/v1/workouts/{id}` would land here too — so it is real provenance.
- **The type stays generic.** One `LandingRecord` in `domain` and one
  `LandingStore` port serve every table; the adapter is constructed against one
  table and the rings above never learn there are several. Adding a source adds
  a migration and an adapter, not a port and not a domain type.

`extraction_run` and `resumption_point` are **not** raw observation data, so
they stay single tables. Both are keyed by a landing stream — `hevy.workouts` —
because a watermark belongs to one table, not to a source: extracting Hevy
workouts and Hevy body measurements resume independently.

```sql
CREATE TABLE extraction_run (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    stream          TEXT    NOT NULL,       -- 'hevy.workouts'
    started_at      TEXT    NOT NULL,
    finished_at     TEXT,
    outcome         TEXT    CHECK (outcome IN ('succeeded', 'failed')),
    events_seen     INTEGER,
    records_landed  INTEGER,
    failure_reason  TEXT,

    -- InFlight is exactly "no outcome yet"; a finished run has all of it.
    CHECK ((outcome IS NULL) = (finished_at IS NULL)),
    CHECK (outcome IS NULL OR (events_seen IS NOT NULL AND records_landed IS NOT NULL)),
    CHECK ((outcome = 'failed') = (failure_reason IS NOT NULL))
);

-- FR-008: the most recent successful extraction, in one indexed lookup.
CREATE INDEX extraction_run_succeeded
    ON extraction_run (stream, finished_at DESC) WHERE outcome = 'succeeded';

CREATE TABLE resumption_point (
    stream      TEXT PRIMARY KEY,
    watermark   TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

-- Raw landing. One table per source and entity type; this is the only one
-- this feature creates.
CREATE TABLE hevy_workout_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,   -- '/v1/workouts/events'
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,   -- Hevy's workout id
    event_kind        TEXT    NOT NULL,   -- verbatim from the source (D12)
    event_time        TEXT,               -- nullable: § 37, D13
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
);

-- The lookup D3 makes on every event: most recent record for this workout.
CREATE INDEX hevy_workout_landing_latest
    ON hevy_workout_landing (source_record_id, id DESC);

-- § II.1: never mutated, compacted or deleted. § X: prefer the mechanism that
-- cannot be talked around. Every landing table carries this pair.
CREATE TRIGGER hevy_workout_landing_is_append_only_update
BEFORE UPDATE ON hevy_workout_landing
BEGIN SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)'); END;

CREATE TRIGGER hevy_workout_landing_is_append_only_delete
BEFORE DELETE ON hevy_workout_landing
BEGIN SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)'); END;
```

Notes:

- **`payload` is `BLOB`, not `TEXT`.** FR-002 says bytes as received. `TEXT`
  would invite an encoding round-trip; SQLite would also happily coerce it.
- **The `CHECK` constraints mirror `RunOutcome`.** The sum type makes the
  invalid combinations unrepresentable in Rust; the constraints make them
  unrepresentable in the file, including to a writer that is not this program.
- **No unique constraint on `source_record_id`.** Many records per workout is
  the normal case, not a violation — that is what append-only means.
- **The append-only triggers are per table**, so every future landing table must
  carry its own pair. That is a real cost of the split and the one thing a new
  source can silently forget — worth a migration checklist, not a shared table.
- **§ 38 costs a union.** "Newest observation per source" reads across landing
  tables rather than filtering one. With `extraction_run` shared and keyed by
  stream, `fitness status` answers without touching them at all.

## Validation rules, and where each is enforced

| Rule | Source | Enforced by |
| --- | --- | --- |
| One record per workout, not per page | FR-001 | `RawPayload` built from a single `RawValue` element (D4) |
| Payload stored uninterpreted | FR-002 | `RawValue` preserves the source span; no re-serialisation |
| Provenance complete | FR-003 | Non-optional fields on `LandingRecord`; construction fails otherwise |
| Deletion is a record, never a removal | FR-004 | `EventKind::Deleted`; append-only triggers |
| Unchanged data lands nothing | FR-005 | Digest comparison against the most recent record (D3) |
| Failed run does not advance the watermark | FR-006 | Watermark written only in the success transaction (D8) |
| Watermark resettable | FR-007 | `reset` deletes the row |
| Last success queryable | FR-008 | `extraction_run` + its partial index |
| Credentials from the environment | FR-009 | `clap(env)`; `.env` gitignored; `secrets` flake check |
| One run at a time | FR-010 | Advisory file lock (D7) |
| Failure distinguishable from an empty success | FR-011 | `RunOutcome` sum type; `events_seen` vs `records_landed` |
