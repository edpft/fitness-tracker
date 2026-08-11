# Implementation Plan: Hevy workout extraction

**Branch**: `001-hevy-workout-extraction` | **Date**: 2026-08-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-hevy-workout-extraction/spec.md`

## Summary

Land every workout Hevy holds into an append-only raw store, permanently
re-derivable and never contacting Hevy again to rebuild what comes after it.
Extract and Load only — no normalisation, no domain entities, no analysis.

The approach: a `fitness extract hevy` command walks
`GET /v1/workouts/events` from a stored resumption point, splits each page into
one record per workout with its bytes untouched, and lands only payloads whose
digest differs from the most recent record for that workout. The resumption
point advances — only on success — to the newest event time the run actually
saw, never to the wall clock, which is the single invariant FR-006 rests on.

Both of the spec's open questions were resolved empirically against the live
account rather than by inference. See [research.md](./research.md):

- **The events feed does reconstruct full history.** 163 distinct `updated` ids
  against a reported `workout_count` of 163, with 160 of those workouts never
  edited and still surfacing as `updated`.
- **No rate limiting is documented, advertised or observable.** 30 rapid
  requests, no `429`, no headers. Backoff is implemented regardless.

Three things the probe found that the published API document does not say, one
of which would have broken every run after the first:

1. **The empty response uses the key `workouts`, not `events`** — and the empty
   response is the steady state for a caught-up run.
2. `superset_id`, not the documented `supersets_id`.
3. Error bodies are not reliably JSON (`401` returns bare `InvalidApiKey`).

## Technical Context

**Language/Version**: Rust 1.95.0, edition 2024 (pinned in `rust-toolchain.toml`)

**Primary Dependencies**: `tokio`, `reqwest` (rustls, no default features),
`serde`/`serde_json` (`RawValue`), `sqlx` (sqlite), `jiff`, `sha2`, `clap`
(derive + env), `thiserror`, `fs4`. Dev: `proptest`, `wiremock`

**Storage**: SQLite, single file, path from configuration. One landing table per
source *and* entity type — `hevy_workout_landing` is the only one this feature
creates — each append-only, enforced by its own `BEFORE UPDATE`/`BEFORE DELETE`
triggers

**Testing**: `cargo nextest` via `nix flake check`; integration tests at port
boundaries with a stub source and a temporary database; `proptest` for § 28;
`wiremock` for the HTTP adapter's contract

**Target Platform**: Linux; deployment-agnostic (§ 34). No path, host or port
compiled in

**Project Type**: Rust workspace, hexagonal. New driving adapter: a CLI

**Performance Goals**: None. Measured: full history is 17 requests in ~10 s.
Growth is one request per ten workouts, hard-capped by `pageSize` ≤ 10

**Constraints**: Payloads byte-identical to what was served (FR-002); raw never
mutated (§ II.1); one run at a time (FR-010); a failed run never advances the
resumption point (FR-006)

**Scale/Scope**: 163 workouts, 164 events, ~1,135 exercises, ~3,779 sets today.
Single user, single operator (§ I)

## Constitution Check

*GATE: passed before Phase 0, re-evaluated after Phase 1 design. Re-evaluation
notes are inline; nothing changed status.*

| Rule | Status | How |
| --- | --- | --- |
| **§ II.1** Raw append-only, unrecognised fields retained | PASS | Bytes stored via `RawValue` with no re-serialisation; database triggers refuse `UPDATE`/`DELETE`; no port exposes a mutation path |
| **§ II** Watermark is reconstructible state, not an input | PASS | Stored as a disposable cache; reset is a row deletion costing one re-fetch and landing nothing |
| **§ II** No layer invented for non-observation data | PASS | Three entities, all named in the spec. No source registry, no catalogue |
| **§ 7** Re-derivation without refetching | PASS | Full bodies landed; SC-005 asserts it |
| **§ 8/§ 10** Identity and supersession | PASS | Source id carried as provenance; supersession explicitly *not* resolved here — serve ordinal is order-of-service, not precedence |
| **§ 15/16** Hexagonal, every external system behind a port | PASS | `WorkoutEventSource`, `LandingStore`, `ResumptionPointStore`, `ExtractionRunLog`, `RunLock`, `Clock`. No `reqwest`, `sqlx` or `jiff` type in a port signature |
| **§ 17** Deterministic first | PASS | No LLM. Nothing here is generative |
| **§ 19** Frontend holds no domain logic | N/A | No frontend this feature |
| **§ 20/21** Rust only; interface languages confined to their adapter | PASS | SQL exists only in the store adapter |
| **§ 23** No raw types at domain boundaries | PASS | Every port parameter is a newtype or sum type ([data-model.md](./data-model.md)) |
| **§ 24** Illegal states unrepresentable | PASS | `RunOutcome` sum type; `PayloadDigest` constructible only by digesting a payload; `CHECK` constraints mirror the sum type in the file |
| **§ 25** Types document | PASS | `EventKind::Unrecognised`, `Watermark`, `RawPayload` need no gloss |
| **§ 26** Errors typed, no panics, vendor errors translated at the port | PASS | `thiserror` throughout; translation table in [contracts/ports.md](./contracts/ports.md). Note `clippy::exit` is `forbid` — `main` returns `ExitCode` |
| **§ 27** Types first | PASS | Task ordering puts the domain vocabulary before any behaviour |
| **§ 28** A random instance of a type is valid | PASS | `proptest` over every newtype; constructors are the only way in |
| **§ 29/30** Integration tests at ports are the primary suite | PASS | Every scenario in [quickstart.md](./quickstart.md) is a port-boundary test with a stub source; no live credential in the suite |
| **§ 31** Red-green-refactor at the port boundary | PASS | Each scenario test precedes its implementation |
| **§ 32/33** Minimal scope, no proof-of-concept code | **PARTIAL** | See Complexity Tracking — `web` is retained with no work to do |
| **§ 34** Deployment-agnostic | PASS | Base URL, database path and credential all configuration |
| **§ 35** Credentials never in version control | PASS | Env only; no CLI flag for the key; `.env` gitignored; `secrets` check |
| **§ 36** A source being unavailable degrades, never fails | PASS | Scenario 7: extract exits `1`, raw unchanged, `status` still answers |
| **§ 37** Partial data recorded as partial | PASS | Event time nullable; no substitution of the fetch time, which would also risk over-advancing the watermark |
| **§ 38** Staleness observable | PASS | `fitness status`; `events_seen` and `records_landed` reported separately (FR-011) |
| **§ 40** Human sign-off before merge | PASS | Branch and PR; not self-merged |

### Conflicts surfaced (Governance)

All three raised during planning and **settled by the operator**. Each resolves
by revising an artifact; none required amending the constitution.

1. **SC-001 fails on a correct run as written.** A first extraction lands 164
   distinct workout ids against a reported count of 163; the extra is a workout
   that exists only as a deletion — correctly landed, correctly absent from
   `workout_count`. **Settled: revise the spec** to count workouts *whose most
   recent landing record is an update rather than a deletion*. Stronger than
   filtering on event kind, which works only while the two id sets are disjoint
   and over-counts once a landed workout is later deleted. Wording and reasoning
   in [research.md](./research.md); the query is in
   [quickstart.md](./quickstart.md).

2. **CLAUDE.md says "`web` is the composition root".** This feature adds a
   second driving adapter. **Settled: revise CLAUDE.md and the README layout
   table** to name two driving adapters, both composition roots. A web interface
   is confirmed as coming; the CLI is what this feature needs. Not a
   constitutional conflict — § 15/16 never required one root.

3. **`crates/domain/Cargo.toml` says "The core depends on nothing."** **Settled:
   the domain may take data-type dependencies** — `jiff`, `sha2`, `uuid`, `url`
   and their kind. What it may not take is a workspace crate, a framework, a
   transport or a store. `flake.nix` already says as much ("chrono or uuid in
   the domain is nobody's emergency"); the `Cargo.toml` comment is reworded to
   match, so the rule is stated where it will be read.

## Project Structure

### Documentation (this feature)

```text
specs/001-hevy-workout-extraction/
├── plan.md              # This file
├── spec.md
├── research.md          # Phase 0 — both open questions resolved empirically
├── data-model.md        # Phase 1
├── quickstart.md        # Phase 1
├── contracts/           # Phase 1
│   ├── ports.md                    # Application ports
│   ├── cli.md                      # Operator-facing command surface
│   ├── hevy-workout-events.md      # The consumed external contract
│   └── hevy-openapi.pinned.json    # What Hevy published on 2026-08-11
├── checklists/
│   └── requirements.md
└── tasks.md             # Phase 2 — /speckit-tasks, not created here
```

### Source Code (repository root)

```text
crates/
├── domain/                 ring 0
│   └── src/
│       ├── lib.rs
│       └── raw/            landing record, provenance value types, watermark
├── application/            ring 1
│   └── src/
│       ├── lib.rs
│       ├── ports.rs        driven + driving ports
│       └── extract.rs      the use case
├── infrastructure/         ring 2
│   └── src/
│       ├── lib.rs
│       ├── hevy/           WorkoutEventSource over reqwest
│       ├── store/          SQLite adapters + migrations
│       └── lock.rs         advisory file lock (FR-010)
├── cli/                    ring 3   NEW — driving adapter, composition root
│   └── src/main.rs         extract | status | reset
└── web/                    ring 3   retained, dormant this feature

migrations/                 sqlx migrations
.sqlx/                      offline query metadata, committed
```

**Structure Decision**: the existing four-crate hexagonal workspace, plus
`crates/cli` at ring 3 as a peer driving adapter to `web`. Extraction is
invoked from a terminal or an external scheduler and never over HTTP; multiple
driving adapters is the ordinary hexagonal arrangement and the constitution
names no single composition root. Operator decision taken during planning with
the alternatives on the table ([research.md](./research.md), D9).

Adding the crate is the documented three-edit process, and the third edit is the
one that gets forgotten:

1. `[workspace.dependencies]` in `Cargo.toml`
2. `workspaceSrc` in `flake.nix` — omit this and cargo is happy while nix
   silently ignores the sources
3. a ring in `crateRings` in `flake.nix` — `cli = 3`

The `workspace-members` and `architecture` checks catch 2 and 3. `cli = 3`
alongside `web = 3` passes the ring check, which requires a strict decrease
across every edge: neither may depend on the other, and neither does.

The example `Item` type is deleted, not extended — it is scaffolding, and this
is the feature that replaces it.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
| --- | --- | --- |
| `web` retained with no work to do (§ 33: no proof-of-concept-grade code) | A web interface is confirmed as coming; the CLI is what this feature needs. Keeping the ring means the first HTTP feature adds no crate. `main` becomes a stub reporting there is no HTTP surface yet | Deleting `web` was offered and declined — it would be re-added shortly. Folding the CLI into `web` was rejected because a batch job invoked as `web extract` is a name that misleads |
| A fifth crate | Extraction has an operator-facing entry point that is not HTTP, and driving adapters are where a transport choice belongs | Subcommands on the `web` binary — see above |

Two further costs, accepted rather than violations:

- **`.sqlx/` offline metadata must be regenerated when a query changes.**
  `cargo sqlx prepare --check` added to the flake makes a stale directory a CI
  failure rather than a mystery. It is an established tool, not a bespoke check
  (§ X).
- **TLS backend licensing.** `cargo-deny` runs a strict allowlist that admits
  neither `OpenSSL` nor `CC0-1.0`. `rustls` may pull in `aws-lc-sys` or `ring`,
  either of which can fail `audit-licenses`. Resolution when the dependency
  lands is a deliberate `[licenses.exceptions]` entry with a recorded reason —
  never a widened `allow` list.

## Post-Design Constitution Re-check

Re-run after Phase 1. No gate changed status. Two observations from designing
the artifacts:

- **The store enforces § II.1 independently of the code.** The append-only
  triggers mean SC-003 is checkable by a stranger with `sqlite3` and no
  knowledge of this repository. § X asks for the mechanism that cannot be talked
  around, and this is one.
- **§ 24 is doing real work on `RunOutcome`.** FR-011 — telling a failed run
  from a successful one that found nothing — stops being a convention to
  remember and becomes a shape the type will not let you build wrongly, mirrored
  by `CHECK` constraints in the file for writers that are not this program.

The one gap that design did not close is the `web` crate's dormancy, recorded
above rather than argued away — time-boxed rather than open-ended, since an HTTP
surface is the next thing that ring will carry.
