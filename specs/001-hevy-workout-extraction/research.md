# Phase 0 Research: Hevy workout extraction

**Feature**: [spec.md](./spec.md) | **Date**: 2026-08-11

Both `[NEEDS CLARIFICATION]` markers in the spec's Open Questions are resolved
here, empirically, against the live account. Neither is left as inference.

The pinned OpenAPI document the API serves is committed alongside this file as
[`contracts/hevy-openapi.pinned.json`](./contracts/hevy-openapi.pinned.json).
It is a record of what the source claimed on this date, not a dependency — and
§ "Observed behaviour" below records three places where it is wrong.

---

## Open Question 1 — Completeness of the events feed

**RESOLVED: the assumption holds. Confirmed against the live account, not inferred.**

**Decision**: `GET /v1/workouts/events?since=1970-01-01T00:00:00Z` is the sole
collection endpoint. `GET /v1/workouts` is not used.

**Evidence**, from a full walk of the feed on 2026-08-11:

| Measurement | Value |
| --- | --- |
| `GET /v1/workouts/count` → `workout_count` | 163 |
| Pages at `pageSize=10` | 17 |
| Events returned | 164 |
| `updated` events / distinct ids | 163 / 163 |
| `deleted` events / distinct ids | 1 / 1 |
| Overlap between the two id sets | 0 |
| Workouts never edited (`created_at == updated_at`) | 160 of 163 |

The 163 distinct `updated` ids equal `workout_count` exactly. 160 of those
workouts have never been edited and still surface as `updated`, which is the
precise claim the spec assumed and could not confirm: **the feed has no
`created` event type, and a workout's creation surfaces as an `updated` event
carrying its full body.** Requesting from the epoch reconstructs the entire
history.

**Rationale for using the feed rather than `GET /v1/workouts`**: only the feed
carries deletions (FR-004). `GET /v1/workouts` serves live workouts alone, so a
deletion would be invisible — detectable only by diffing against what was
previously landed, which is raw consulting its own history and forbidden by
FR-002.

**Alternatives considered**:

- *`GET /v1/workouts`, paginated* — rejected: no deletions, and the same
  pagination cost. Retained as the contingency if the feed ever regresses;
  SC-001 is the detector.
- *Per-workout `GET /v1/workouts/{id}`* — rejected: requires knowing the id
  set, which only the feed or the list endpoint provides.

### A delete-only event exists in this account today

The single `deleted` event's id appears in no `updated` event. The spec's edge
case — *"a delete event arrives for a workout that was never landed"* — is
therefore **live on the very first run**, not hypothetical. The specified
behaviour (land it anyway; resolve at the canonical layer) is exercised by real
data on day one, and the quickstart asserts it.

### The feed is one row per workout, not a log of events

The zero overlap between the two id sets is evidence about the feed's shape, and
it settles what happens when a workout is deleted.

If the feed were an append-only event log, a deleted workout would keep its
`updated` row and gain a `deleted` row alongside it. It does not: the workout
deleted on 2025-11-05 has *only* a `deleted` row, despite certainly having been
created and served as `updated` before then. **Deletion replaces the workout's
row rather than adding to it.** The feed carries current state per workout —
164 rows for 164 workouts the account has ever held — and `since` filters on
whichever timestamp that row currently bears.

**Prediction**: deleting one more workout yields 162 `updated` + 2 `deleted`,
still 164 events, with `workout_count` falling to 162. Untested — it needs a
deletion, and the API has no `DELETE` endpoint (`POST`, `PUT` and `GET` only),
so it can only be done in the Hevy app. The quickstart carries it as a deferred
live check with a disposable workout, so no real training record is risked.

**Consequences, none of which change the design**:

- *Incremental detection still works.* A deletion rewrites the row's timestamp
  to `deleted_at`, promoting it to the top of the feed, so a run resuming from
  the watermark sees it (FR-004).
- *The spec already anticipated this.* Its edge case reads: "If the source
  collapses repeat edits into a single current state, one record results and no
  edit is lost that the source still holds — raw cannot land what it was never
  served." That is exactly the observed shape.
- *Deleted workouts persist in the feed.* This one has for nine months, so a
  reset re-collects deletions along with everything else, and deduplication
  means the re-fetch lands nothing (FR-005, scenario 6).
- *Our raw store is the append-only log the feed is not.* A workout landed as
  `updated` and later landed as `deleted` keeps both records — which is the
  whole point of § II.1, and why SC-001 must be phrased against the *most
  recent* record rather than against event kind alone.

### This breaks SC-001 as literally written — spec revision required

SC-001 says *"the number of distinct workouts landed equals the count the
source independently reports"*. A first run lands **164** distinct workout ids
against a reported count of **163**. The extra id is the delete-only workout,
which is correctly landed and correctly absent from `workout_count`.

Taken literally, SC-001 fails on a correct run. **Agreed revision** (operator,
during planning):

> **SC-001**: After a first full extraction, the number of distinct workouts
> landed **whose most recent landing record is an update rather than a
> deletion** equals the count the source independently reports for the account.

"Most recent record", not "kind is `updated`". The weaker phrasing happens to
work on a first run, because the two id sets are disjoint there — but once a
landed workout is later deleted, raw holds both an `updated` and a `deleted`
record for it, and counting by kind would over-count from then on. The stronger
phrasing holds at any point in the account's life, which is what makes it worth
asserting on every run rather than once.

Using "most recent record" in a *validation query* is not raw resolving
supersession — § 10 keeps that at the canonical layer. It is a probe, in the
same class as the `json_extract` check in the quickstart.

`GET /v1/workouts/count` is the "independent report" SC-001 refers to — it
exists, and it is the assertion the quickstart makes. The spec should be amended
to match before implementation.

---

## Open Question 2 — Rate limits

**RESOLVED: no throttling is documented, advertised, or observable. Design
defensively anyway.**

**Evidence**:

- The OpenAPI document declares no `429` on any operation. Documented response
  codes across all 22 endpoints: `200`, `201`, `400`, `403`, `404`, `409`, `500`.
- 30 rapid sequential `GET /v1/workouts/count` requests: all `200`, no failures,
  15.6 s wall clock.
- No `x-ratelimit-*`, `retry-after`, or equivalent header on any response.
- Per-request latency 0.36–4.85 s (median ≈0.5 s). The 4.85 s outlier is
  ordinary Heroku tail latency, not a throttle signal.

**Decision**: treat `429` and `5xx` as retryable with bounded exponential
backoff plus jitter, honouring `Retry-After` when present; treat `4xx` other
than `429` as terminal. Pace is configurable but unthrottled by default.

**Rationale**: an absence of documented limits is not a guarantee of none, and
backoff costs nothing if the source never throttles. The cost of being wrong in
the other direction is a run that fails partway — which is safe (FR-006) but
wasteful.

**Scale, measured**: 163 workouts is 17 requests and ~10 s. The spec's volume
assumption ("a few dozen requests") is confirmed; its "few thousand workouts"
is an over-estimate by an order of magnitude. No pacing, streaming or bulk
strategy is warranted. `pageSize` is hard-capped at 10 (`400` above it), so
request count grows linearly at one per ten workouts — ~1,000 requests would
require ~10,000 workouts, decades away at this account's rate.

---

## Observed behaviour that the OpenAPI document gets wrong

Three discrepancies between the published spec and the live API. All three were
found by probing; none would have been caught by reading the document. They are
the concrete justification for § II.1 — raw exists because translation of a
format we do not control is fallible.

### 1. The empty result uses a different key — `workouts`, not `events`

This is the highest-risk finding in this document.

```jsonc
// Populated response
{"page":1,"page_count":17,"events":[ … ]}

// Empty response — note the key
{"page":1,"page_count":1,"workouts":[]}
```

Reproduced with `since=2099-01-01T00:00:00Z` and `since=2026-08-11T00:00:00Z`.
The declared `PaginatedWorkoutEvents` schema marks `events` as **required**, so
a deserialiser written from the schema fails on this response.

**Why it matters**: the empty response is the steady state. Every repeat run
once extraction has caught up returns exactly this — which is acceptance
scenario 2 (*"Repeat run, nothing changed"*), FR-005, and SC-002. A strict
parser would pass the first run and fail every run thereafter.

**Decision**: the adapter treats a response bearing neither key, or bearing
`workouts` instead of `events`, as **zero events** — not as an error. Both
shapes deserialise to an empty page. A contract test pins this against a
recorded response so a future Hevy fix in either direction is caught.

### 2. `superset_id`, not `supersets_id`

The `Workout` schema declares `supersets_id` on each exercise. All 1,135
exercises in the live corpus carry `superset_id` (singular). This costs this
feature nothing — payloads are stored as received — but it is a dependency the
normalisation feature inherits, alongside the exercise-type metadata already
noted in the spec's Open Questions.

### 3. Error bodies are not always JSON

`401` returns the bare string `InvalidApiKey`, not a JSON object, where `400`
returns `{"error":"…"}`. The adapter must not assume a JSON error body.

### Field presence in the live corpus

Every documented field was present on all 163 workouts, 1,135 exercises and
3,779 sets; absent values are `null` rather than omitted. This says nothing
about what the source may serve tomorrow, and changes nothing here — payloads
are stored byte-for-byte regardless.

---

## Confirmed request/response contract

| Property | Observed |
| --- | --- |
| Base URL | `https://api.hevyapp.com/v1` — no `servers` block in the document, so this is configuration (§ 34) |
| Auth | `api-key` request header, UUID format. Hevy Pro only |
| Auth failure | `401`, body `InvalidApiKey` (plain text) |
| `since` default | `1970-01-01T00:00:00Z` |
| **`since` boundary** | **Inclusive** — an event whose timestamp equals `since` is returned |
| `since` filters on | event time: `workout.updated_at` for updates, `deleted_at` for deletes |
| Ordering | newest → oldest, verified across all 164 events |
| `pageSize` | default 5, max 10; `400` above 10 |
| `page` beyond `page_count` | `404` `{"error":"Page not found"}` |

**`since` is inclusive** — verified by requesting `since` equal to the third
newest event's `updated_at` and receiving exactly 3 events, the boundary event
among them. This removes the need for a defensive epsilon on the watermark: a
run may set the watermark to the newest event time it saw and re-request from
exactly there, and no same-timestamp sibling can be skipped. The boundary event
is re-served and deduplicated (FR-005), which is free.

---

## Design decisions

### D1 — Watermark is the newest event time *seen in the run*, never "now"

**Decision**: on success, the resumption point advances to the maximum event
time across events the run actually saw. If the run saw no events, it is left
unchanged. It is never set from the clock.

**Rationale**: this is the invariant that makes concurrent modification safe.
The feed orders newest-first, so a workout edited *during* a run is promoted to
the top of the feed and can be displaced past a page the run has already read.
Because the watermark never advances beyond an event the run observed, and that
edit's timestamp is by definition newer, the next run collects it. Setting the
watermark to the wall clock would step over it permanently and silently — the
exact failure FR-006 exists to prevent.

Insertions at the head of the feed shift unread items *down*, so a walk from
page 1 to `page_count` can duplicate but never skip; duplicates deduplicate.

**Alternatives considered**: watermark from the clock (rejected — silent loss
as above); watermark derived from raw on demand (rejected — a partial run's
records are landed and durable, so a derived watermark would advance on
failure, breaking FR-006).

### D2 — The watermark is stored, and storage is what makes it disposable

**Decision**: one row per source in a `resumption_point` table. Reset (FR-007)
is deletion of that row; the next run then requests from the epoch.

**Rationale**: § II classifies it as reconstructible state governed by nothing —
losing it costs a re-fetch, never a fact. Deleting the row costs exactly one
re-fetch of 17 requests, and FR-005 deduplication means the re-fetch lands
nothing. It cannot be *derived* rather than stored, because FR-006 requires a
failed run to leave it unmoved while that run's records are already durable.

### D3 — Change detection: digest of the exact bytes, against the most recent record

**Decision**: land an event only when the SHA-256 of its payload bytes differs
from the digest of the most recent landing record for that
`source_record_id` within the stream's landing table. No prior record means land it.

**Rationale**: FR-005 and scenario 6 both compare against *the most recent*
record, not against any record — a workout edited to X, then Y, then back to X
lands three records, which is correct: the source served three payloads. The
digest is of the bytes as received, with no canonicalisation, because
canonicalising is interpretation and FR-002 forbids it. A serializer change at
the source would produce one spurious duplicate per workout, once — harmless,
because § 10 makes the later record supersede at the canonical layer anyway.

**Alternatives considered**: canonical-JSON digest (rejected — interpretation,
guarding against a failure mode that is harmless by construction); full byte
comparison (rejected — the digest is what an index can be built on).

### D4 — Exact bytes preserved with `serde_json::value::RawValue`

**Decision**: deserialise the page envelope (`page`, `page_count`, `events`)
with `events: Vec<Box<RawValue>>`, and store each element's original bytes
verbatim. Read `type`, the workout id and the event time out of each event for
provenance columns, without re-serialising the stored payload.

**Rationale**: FR-001 requires one record per workout, and FR-002 requires the
payload as received. Splitting the page is unavoidable, and re-serialising a
parsed `Value` would change the bytes — dropping unrecognised fields, reordering
keys, and normalising numbers, all forbidden by § II.1. `RawValue` borrows the
exact source span, so the envelope is parsed while the event bodies are not.

### D5 — Storage: SQLite via `sqlx`

**Decision**: SQLite, one file, path from configuration. Access through `sqlx`
with compile-time-checked queries and offline metadata (`.sqlx/`) committed.

**Rationale**: single user, single operator, 163 workouts — a server-based store
buys nothing and costs an operational dependency (§ 32). The decisive argument
is § VII.29 read against the gate: integration tests at port boundaries are the
primary suite, and `nix flake check` runs them in a hermetic sandbox with no
network and no services. SQLite runs there against a temporary file; Postgres
would need a server inside the sandbox. `sqlx`'s `prepare --check` is an
established tool that verifies SQL against the schema at build time, which is
what § X asks for.

**Alternatives considered**: Postgres (rejected — operational weight, and
sandboxed integration tests become a harness problem); `rusqlite` (rejected —
synchronous, so the store port would need re-declaring when the `web` ring
acquires an HTTP surface).

**Cost accepted**: `.sqlx/` metadata must be regenerated when a query changes.
`cargo sqlx prepare --check` in the flake makes a stale directory a CI failure
rather than a mystery.

### D6 — Raw is append-only, enforced by the database

**Decision**: `BEFORE UPDATE` and `BEFORE DELETE` triggers on the landing table
that `RAISE(ABORT)`.

**Rationale**: § II.1 says landing records are never mutated, compacted or
deleted. § X prefers the mechanism that cannot be talked around. A trigger is
enforced against every writer including a stray `sqlite3` session, where a
code-level convention is enforced only against code that remembers it.

### D7 — Concurrency: an OS advisory file lock

**Decision**: `try_lock_exclusive` on a lock file beside the database, taken at
run start. Failure to acquire ends the run immediately with a distinct exit
code, no landing records and no watermark movement (FR-010).

**Rationale**: the lock is released by the kernel when the process dies, so a
crashed run leaves nothing to unstick. Alternatives that record in-flight state
in the database — a `running` row under a unique partial index — survive process
death and require a manual recovery step, which is a worse failure mode for a
single operator.

**Limitation accepted**: advisory locks are per-machine and unreliable over NFS.
Single user, single machine (§ I); recorded so it is a known boundary rather
than a surprise.

### D8 — Landing commits per page, not per run

**Decision**: each page's records commit in their own transaction. The watermark
advances and the run is marked successful in a final transaction.

**Rationale**: a run that fails on page 16 of 17 keeps 15 pages of work. The
retry re-fetches from the unmoved watermark and deduplicates what is already
landed (FR-005), so the observable result is identical to an uninterrupted run
(SC-004) at a fraction of the cost. Records landed by a failed run persist and
must persist — deleting them would violate § II.1.

**Alternative considered**: one transaction for the whole run (rejected —
simpler, but discards the entire fetch on any failure, and would have made the
FR-010 lock and the write transaction the same object, which then blocks its own
per-page writes).

### D9 — Crate topology: a `cli` crate at ring 3, `web` parked

**Decision**: add `crates/cli` at ring 3, a peer driving adapter to `web`.
Extraction is invoked from a terminal or an external scheduler, never over HTTP.

**Rationale**: multiple driving adapters is the ordinary hexagonal arrangement
(§ 15, § 16), and nothing in the constitution names a single composition root.
Operator decision, taken during planning with the alternatives on the table.

**Consequences**: `web` keeps its ring but does nothing this feature — recorded
in the plan's Complexity Tracking rather than glossed. CLAUDE.md's "`web` is the
composition root" and the README layout table both need a line acknowledging two
driving adapters.

**Alternatives considered**: CLI subcommands on the `web` binary (rejected —
zero churn, but a batch job invoked as `web extract` is a name that misleads);
deleting `web` until an HTTP feature needs it (offered, declined).

### D10 — Time: `jiff`

**Decision**: `jiff` for all timestamps. This feature stores UTC instants only.

**Rationale**: § II requires timestamps to carry an IANA zone identifier and
states that an offset is not a substitute. `jiff`'s `Timestamp` (instant) and
`Zoned` (instant + IANA zone) map onto exactly that distinction, and it has no
naive local type to reach for by accident. `chrono` + `chrono-tz` is more
widely used but makes a § II violation the path of least resistance. Nothing
here needs zone handling yet; the normalisation feature inherits the choice.

**Storage form**: RFC 3339 UTC strings in `TEXT` columns. SQLite has no
timestamp type, and `sqlx` has no `jiff` integration, so the conversion is
explicit at the adapter — which is where a vendor representation belongs.

### D11 — Webhooks stay out of scope, and could not replace this feature

Raised during planning. Hevy does offer a webhook: it `POST`s
`{"workoutId": "…"}` to a URL on workout creation, expecting `200` within 5 s.

**Decision**: out of scope, as the spec already has it. Recorded here because
the reason is structural rather than a matter of sequencing.

- **It fires on creation only.** No edit or delete notification, so FR-004 and
  acceptance scenario 3 are unreachable through it. The events feed remains
  mandatory regardless.
- **It carries no observation.** The payload is an id; landing anything would
  require a follow-up fetch. It is a *trigger*, not a source.
- **It requires a publicly reachable inbound endpoint** with a 5-second budget —
  a deployment constraint (§ 34) on a system that currently has no HTTP surface.
- **It cannot be relied on.** A missed delivery is undetectable without the
  polling path that this feature builds.

Its natural home is the freshness-policy feature the spec defers ("extraction is
invoked, not self-triggering") — as an optimisation that lowers latency on new
workouts, on top of extraction, never instead of it.

### D12 — Unrecognised event kinds

**Decision**: an event whose `type` is neither `updated` nor `deleted` is landed
with its payload intact and its kind recorded verbatim. The source workout id is
read from `workout.id` or `id`, whichever is present. If neither is present, the
run fails visibly rather than landing a record without provenance.

**Rationale**: § II.1 forbids discarding what is not recognised, and FR-003
requires every record to carry the source's identifier — a record that cannot
satisfy FR-003 is a failure to surface, not a record to weaken the rule for.

### D13 — Partial provenance is recorded as partial

**Decision**: the event-time column is nullable. `deleted_at` is optional in the
declared schema, and `updated_at` is not marked required. An event with no
timestamp is landed with a null event time and contributes nothing to the
watermark.

**Rationale**: § 37 — partial data is recorded as partial, with no gap-filling
on write. Substituting the fetch time would be exactly the silent carry-forward
that rule forbids, and would risk advancing the watermark past unseen events.
Every event in the live corpus carries a timestamp; this is defence against a
future one that does not.

---

## Dependencies

| Crate | Purpose | Ring |
| --- | --- | --- |
| `jiff` | Timestamps (D10) | domain |
| `sha2` | Payload digest (D3) | domain |
| `thiserror` | Typed errors (§ 26) | all |
| `serde`, `serde_json` | Envelope parsing, `RawValue` (D4) | infrastructure |
| `reqwest` (`rustls-tls`, no default features) | Hevy adapter | infrastructure |
| `tokio` | Async runtime | infrastructure, cli |
| `sqlx` (`sqlite`, `runtime-tokio`) | Store adapter (D5) | infrastructure |
| `fs4` | Advisory run lock (D7) | infrastructure |
| `clap` (`derive`, `env`) | Command surface, env-backed config (§ 34, § 35) | cli |
| `proptest` (dev) | § 28 validity of generated instances | domain |
| `wiremock` (dev) | HTTP contract tests against recorded responses | infrastructure |

`rustls` rather than `native-tls`: no OpenSSL, so the nix build stays hermetic
with no `buildInputs` change.

**Known risk — TLS backend licensing.** `cargo-deny` runs with a strict
allowlist that does not include `OpenSSL` or `CC0-1.0`. Depending on which
backend `rustls` pulls in, `aws-lc-sys` (`ISC AND (Apache-2.0 OR ISC) AND
OpenSSL`) or `ring` (no SPDX expression; uses `license-file`) may fail
`audit-licenses`. This is discovered the moment the dependency lands, and the
resolution is a deliberate `[licenses.exceptions]` or `[licenses.clarify]` entry
in `deny.toml` with a recorded reason — not a widened `allow` list. Flagged so
it is a five-minute task rather than a surprise CI failure.

**Trap — `clippy::exit` is `forbid`.** `std::process::exit` cannot be called,
and no `#[allow]` can rescue it (E0453). The CLI returns
`std::process::ExitCode` from `main`. Distinct exit codes are part of the
command contract.

## Configuration (§ 34, § 35)

| Variable | Purpose | Default |
| --- | --- | --- |
| `HEVY_API_KEY` | Credential. Never committed; `.env` is already gitignored | none — required |
| `HEVY_API_BASE_URL` | Source base URL | `https://api.hevyapp.com/v1` |
| `FITNESS_TRACKER_DATABASE` | SQLite file path | none — required |

No path, host or port is compiled in. The base URL is configuration because the
OpenAPI document declares no `servers` block, and because the contract tests
point it at a local stub.
