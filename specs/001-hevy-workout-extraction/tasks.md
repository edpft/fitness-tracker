# Tasks: Hevy workout extraction

**Input**: Design documents from `/specs/001-hevy-workout-extraction/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/)

**Tests**: Mandatory, not optional. The template treats tests as opt-in; the constitution does not. § 29 makes integration tests at port boundaries "the primary suite and the agent's steering signal", § 31 requires red-green-refactor at that boundary, § 28 requires property tests asserting that a generated instance of a type is valid, and § 30 requires the public API to be fully tested. Test tasks are ordered before the implementation they steer.

**Organization**: One user story. The spec says why it does not decompose: a partial history in raw satisfies no downstream need, so slicing it would produce fragments that deliver value only once all are present.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1)
- Exact file paths included in every task

## Path Conventions

Rust workspace, hexagonal, dependencies inward only:
`cli → infrastructure → application → domain`, with `web` a peer of `cli` at ring 3.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Bring the workspace to the shape the plan describes, and settle the documentation conflicts planning surfaced.

- [X] T001 Create `crates/cli/Cargo.toml` (name `cli`, binary `fitness`, workspace lints, depends on `application`, `infrastructure`, `domain`) and a placeholder `crates/cli/src/main.rs` returning `std::process::ExitCode`
- [X] T002 Add `cli` to `workspaceSrc` in `flake.nix` via `craneLib.fileset.commonCargoSources ./crates/cli` — omit this and cargo is happy while nix silently ignores the sources
- [X] T003 Add `cli = 3;` to `crateRings` in `flake.nix`, and add a `buildPackage` block for `cli` with `meta.mainProgram = "fitness"`, plus a `checks` entry
- [X] T004 Verify `nix flake check` passes the `workspace-members` and `architecture` checks with the new crate. `members = ["crates/*"]` globs it into cargo automatically; the two flake edits above are what nix needs
- [X] T005 [P] Add workspace dependencies to the root `Cargo.toml`: `tokio`, `reqwest` (no default features, `rustls-tls`), `serde`, `serde_json` (`raw_value`), `sqlx` (`sqlite`, `runtime-tokio`, `migrate`), `jiff`, `sha2`, `clap` (`derive`, `env`), `thiserror`, `fs4`, and dev-dependencies `proptest`, `wiremock`, `tempfile`
- [X] T006 [P] Run `nix flake check` and resolve the `audit-licenses` outcome for the TLS backend. If `ring` or `aws-lc-sys` fails the allowlist, add a `[licenses.exceptions]` or `[licenses.clarify]` entry to `deny.toml` with a recorded reason — never widen `allow`
- [X] T007 [P] Amend **SC-001** in `specs/001-hevy-workout-extraction/spec.md` to read "the number of distinct workouts landed whose most recent landing record is an update rather than a deletion equals the count the source independently reports". As written it fails a correct run — see research.md
- [X] T008 [P] Update `CLAUDE.md` and `README.md`: `web` is no longer the sole composition root, and the layout table gains `cli` as a second driving adapter
- [X] T009 [P] Reword the comment in `crates/domain/Cargo.toml` from "The core depends on nothing" to state the actual rule: no workspace crates, frameworks, transports or stores; data-type dependencies (`jiff`, `sha2`, `uuid`, `url`) are allowed

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Remove the scaffolding this feature replaces and stand up the store plumbing every later task writes against.

**⚠️ CRITICAL**: No user story work can begin until this phase is complete.

- [X] T010 Delete the `Item` example end to end: `crates/domain/src/lib.rs` (`Item`, `ItemId`, `InvalidItem` and their tests), `crates/application/src/lib.rs` (`ItemRepository`, `CreateItem`, `ItemService`, `RepositoryError`, `CreateItemError` and their tests), `crates/infrastructure/src/lib.rs` (`InMemoryItemRepository` and its test). It is scaffolding to be deleted, not extended
- [X] T011 Replace `crates/web/src/main.rs` with a stub returning `ExitCode` and reporting that no HTTP surface exists yet, so the crate compiles without the `Item` wiring
- [X] T012 Create `migrations/` and wire `sqlx::migrate!` into a pool constructor in `crates/infrastructure/src/store/pool.rs`, reading the database path from a parameter — no hardcoded path (§ 34)
- [X] T013 Set `SQLX_OFFLINE` in `flake.nix` and carry `migrations/` and `.sqlx/` in the build's fileset. No separate check is needed after all: an offline build *is* the check, because a query changed without regenerating its metadata fails to compile. A dedicated `prepare --check` job would have needed a live database inside the sandbox to tell us the same thing
- [X] T014 [P] Create the test-double module `crates/application/tests/support/mod.rs` with in-memory fakes for every driven port, plus a fixed `Clock`. Fakes go here, not in `infrastructure` — a use-case test that needs a database has a dependency pointing the wrong way

**Checkpoint**: workspace builds green with no example code; store plumbing and fakes ready.

---

## Phase 3: User Story 1 — Land the full Hevy history (Priority: P1) 🎯 MVP

**Goal**: Run extraction and have every workout Hevy holds landed in raw, permanently re-derivable without contacting Hevy again.

**Independent Test**: Run extraction against the live account and confirm the landed count matches what Hevy independently reports (SC-001), then run it again and confirm no records are added (SC-002).

### Domain vocabulary — types first (§ 27)

- [X] T015 [P] [US1] Create `crates/domain/src/landing/ids.rs`: `SourceName`, `Endpoint`, `SourceRecordId`, `EntityKind`, `LandingStream` newtypes, each validating at construction. `SourceRecordId` is **not** parsed as a UUID — validating a source's id format is interpretation (FR-002) and would fail extraction to defend a constraint we do not own
- [X] T016 [P] [US1] Create `crates/domain/src/landing/payload.rs`: `RawPayload` (non-empty bytes) and `PayloadDigest` ([u8; 32]). The ordinary constructor digests a `RawPayload`; a separate `from_storage` exists for the persistence boundary alone, since the store must rehydrate a digest it previously wrote. What the type guarantees everywhere is width
- [X] T017 [P] [US1] Create `crates/domain/src/landing/event.rs`: `EventKind` sum type (`Updated` | `Deleted` | `Unrecognised(RawEventKind)`) and `RawEventKind`. The `Unrecognised` variant is deliberate — a kind Hevy adds later is unknown, not illegal, and § II.1 forbids discarding it
- [X] T018 [P] [US1] Create `crates/domain/src/landing/time.rs`: `FetchedAt`, `EventTime`, `Watermark` over `jiff::Timestamp`
- [X] T019 [US1] Create `crates/domain/src/landing/record.rs`: `LandingRecord` with complete provenance, no setters and no `&mut` accessors. Construction fails without a source record id (FR-003). Depends on T015–T018
- [X] T020 [US1] Create `crates/domain/src/landing/run.rs`: `RunId`, `ExtractionRun`, `RunOutcome` sum type (`InFlight` | `Succeeded { finished_at, events_seen, records_landed }` | `Failed { finished_at, reason }`) and `FailureReason`. Counts live inside `Succeeded`, not as optional fields beside it, so a run cannot report landed records without having finished
- [X] T021 [US1] Declare the module tree in `crates/domain/src/landing/mod.rs` and re-export from `crates/domain/src/lib.rs`
- [X] T022 [P] [US1] Property tests in `crates/domain/tests/value_types.rs`: a generated instance of every newtype is valid, and construction rejects every invalid input (§ 28). If an arbitrary instance can violate an invariant, fix the type, not the generator
- [X] T023 [P] [US1] Property test in `crates/domain/tests/digest.rs`: digesting identical bytes yields identical digests and differing bytes differ — the property FR-005 rests on

### Ports (§ 16) — declared before anything implements them

- [X] T024 [US1] Create `crates/application/src/ports.rs` with the driven ports from [contracts/ports.md](./contracts/ports.md): `WorkoutEventSource`, `LandingStore`, `ResumptionPointStore`, `ExtractionRunLog`, `RunLock`, `Clock`, plus `EventPage` and `SourceEvent`. Use `fn … -> impl Future<Output = …> + Send`, not bare `async fn` — a future with no `Send` bound cannot be held across an `await` in an axum handler later
- [X] T025 [US1] Add the driving ports `ExtractWorkouts`, `ReportExtractionStatus`, `ResetResumptionPoint` to `crates/application/src/ports.rs`
- [X] T026 [US1] Create `crates/application/src/error.rs`: `ExtractionError`, `SourceError`, `StoreError`, `RunLockError`, `StatusError` via `thiserror`. No vendor type appears — no HTTP status, no SQL code (§ 26)
- [X] T027 [US1] `LandingStore` takes no stream parameter: each instance is bound to one landing table at construction, so a store for `hevy.workouts` cannot read another stream. Reflect this in `crates/application/src/ports.rs`

### Integration tests at the port boundary — write these first, watch them fail (§ 29, § 31)

- [X] T028 [P] [US1] `crates/application/tests/extraction.rs` scenario 1: stub serves 17 pages / 164 events; assert every workout landed, `records_landed == 164`, watermark equals the newest event time
- [X] T029 [P] [US1] `crates/application/tests/extraction.rs` scenario 2 (**SC-002**): run twice against an unchanged stub; assert the second run lands zero records and the total is unchanged
- [X] T030 [P] [US1] `crates/application/tests/extraction.rs` scenario 3: stub re-serves one workout with a changed body; assert a second record for that id and that the first is byte-identical and still retrievable
- [X] T031 [P] [US1] `crates/application/tests/extraction.rs` scenario 4: stub serves a `deleted` event for a landed workout; assert a record with kind `deleted` and that the earlier record is present and unaltered
- [X] T032 [P] [US1] `crates/application/tests/extraction.rs` scenario 5 (**SC-004**): stub fails on page 9 of 17; assert the run fails, pages 1–8 are durable and the watermark is unmoved; then rerun against a healthy stub and assert the same end state as one clean run
- [X] T033 [P] [US1] `crates/application/tests/extraction.rs` scenario 6: land fully, reset, run again; assert zero new records for identical payloads, and exactly one new record when a payload differs
- [X] T034 [P] [US1] `crates/application/tests/extraction.rs` scenario 7: stub returns connection errors; assert zero landing records, watermark unmoved, and that status still answers (§ 36)
- [X] T035 [P] [US1] `crates/application/tests/edge_cases.rs`: delete for a workout never landed is landed anyway; multiple edits between runs land every payload served in the order served; empty account succeeds having landed nothing; an unrecognised event kind is landed with its kind verbatim; an event with no timestamp lands with a null event time and does not move the watermark
- [X] T036 [P] [US1] `crates/application/tests/watermark.rs`: the invariant the whole feature rests on — a workout edited mid-run and promoted past an already-read page is collected by the *next* run, because the watermark never advances beyond an event the run actually saw. Assert the watermark is never set from the clock

### Store adapters

- [X] T037 [US1] Write `migrations/0001_extraction.sql` per [data-model.md](./data-model.md): `extraction_run` (keyed by `stream`, with the `CHECK` constraints mirroring `RunOutcome`), `resumption_point`, and `hevy_workout_landing` with no `source` column — the table name carries it
- [X] T038 [US1] Add the append-only triggers to `migrations/0001_extraction.sql`: `BEFORE UPDATE` and `BEFORE DELETE` on `hevy_workout_landing` raising `'raw landing is append-only (constitution II.1)'`. Every future landing table needs its own pair — this is the cost of the per-stream split
- [X] T039 [US1] Add the indexes to `migrations/0001_extraction.sql`: `hevy_workout_landing_latest` on `(source_record_id, id DESC)` for the D3 lookup, and the partial `extraction_run_succeeded` index for FR-008
- [X] T040 [P] [US1] `crates/infrastructure/tests/store.rs`: assert `UPDATE` and `DELETE` against `hevy_workout_landing` are refused by the database (**SC-003**), that the `RunOutcome` `CHECK` constraints reject invalid combinations, and that `latest_digest` returns the most recent record rather than any record
- [X] T041 [US1] Implement `LandingStore` in `crates/infrastructure/src/store/landing.rs`, bound to one table at construction. Timestamps convert to RFC 3339 UTC `TEXT` here; `payload` is `BLOB`
- [X] T042 [P] [US1] Implement `ResumptionPointStore` in `crates/infrastructure/src/store/resumption.rs`, where `clear` deletes the row (FR-007)
- [X] T043 [P] [US1] Implement `ExtractionRunLog` in `crates/infrastructure/src/store/run_log.rs`, including `latest_success` for FR-008
- [X] T044 [US1] Translate `sqlx::Error` into `StoreError` at the adapter boundary in `crates/infrastructure/src/store/mod.rs` — no SQL code crosses inward (§ 26)

### Hevy adapter

- [X] T045 [P] [US1] `crates/infrastructure/tests/hevy_contract.rs`: **the empty response uses the key `workouts`, not `events`** — assert `{"page":1,"page_count":1,"workouts":[]}` yields zero events and a successful run, not a parse error. This is the steady state for every caught-up run, so getting it wrong passes run 1 and fails every run after
- [X] T046 [P] [US1] `crates/infrastructure/tests/hevy_contract.rs` via `wiremock`: a populated page splits into one record per event with bytes unchanged; `since` is passed through unmodified; pagination walks `1..=page_count` and never requests `page_count + 1` (which returns `404`)
- [X] T047 [P] [US1] `crates/infrastructure/tests/hevy_contract.rs`: `401` is terminal and never retried; `429` and `5xx` retry with backoff then surface as `SourceError::Unavailable`; a non-JSON error body yields `Malformed` rather than panicking — `401` returns bare `InvalidApiKey`
- [X] T048 [US1] Implement page deserialisation in `crates/infrastructure/src/hevy/page.rs` using `events: Vec<Box<RawValue>>` so each event's original bytes are preserved exactly (FR-002). Tolerate `events`, `workouts`, or neither key. Never re-serialise a parsed `Value` — that would reorder keys and drop unrecognised fields
- [X] T049 [US1] Extract provenance in `crates/infrastructure/src/hevy/page.rs`: `type`, then the id from `workout.id` (updated) or `id` (deleted), and the event time from `updated_at` or `deleted_at`. An event with neither id fails the run visibly rather than landing a record without provenance (FR-003)
- [X] T050 [US1] Implement `WorkoutEventSource` in `crates/infrastructure/src/hevy/client.rs` over `reqwest`, sending the `api-key` header and `pageSize=10`, with base URL injected
- [X] T051 [US1] Implement bounded exponential backoff with jitter in `crates/infrastructure/src/hevy/retry.rs`, honouring `Retry-After` when present, retrying `429` and `5xx` only, and translating exhaustion into `SourceError::Unavailable`
- [X] T052 [P] [US1] Implement `RunLock` in `crates/infrastructure/src/lock.rs` with `std::fs::File::try_lock` on a lock file beside the database — std grew file locking in Rust 1.89, so the planned `fs4` dependency was dropped, and `crates/infrastructure/tests/run_lock.rs` asserting a second acquisition fails immediately and that the lock is released when the holding process dies

### The use case

- [X] T053 [US1] Implement `ExtractWorkouts` in `crates/application/src/extract.rs`, generic over its ports: acquire the lock, begin the run, read the watermark, walk pages `1..=page_count`, deduplicate by digest against the most recent record, commit per page, then advance the watermark and record success in a final transaction
- [X] T054 [US1] Implement watermark advancement in `crates/application/src/extract.rs`: the maximum event time across events the run **saw** (landed or deduplicated), never the clock, and unchanged when the run saw no events. Pass the stored watermark to `since` unmodified — it is inclusive at the source, so the boundary event is re-served and deduplicated for free
- [X] T055 [US1] Implement failure handling in `crates/application/src/extract.rs`: roll back nothing already committed (§ II.1 — landed records persist), record the failed run in its own transaction so FR-011 stays answerable, and leave the watermark unmoved (FR-006)
- [X] T056 [P] [US1] Implement `ReportExtractionStatus` and `ResetResumptionPoint` in `crates/application/src/status.rs`, reporting `events_seen` and `records_landed` separately and rendering "never run" as a fact rather than an error

### CLI

- [X] T057 [US1] Implement the command surface in `crates/cli/src/main.rs` per [contracts/cli.md](./contracts/cli.md): `extract <source>`, `status`, `reset <source>`. Built with clap's **builder** API, not derive: the derive macros expand with `#[allow(clippy::restriction)]`, and an allow for a forbidden lint is a compile error (E0453)
- [X] T058 [US1] Implement configuration in `crates/cli/src/config.rs`: `HEVY_API_KEY` from env with **no flag** — a credential on the command line lands in shell history and `ps` output (§ 35) — plus `HEVY_API_BASE_URL` and `FITNESS_TRACKER_DATABASE` with flags
- [X] T059 [US1] Wire the composition root in `crates/cli/src/main.rs`: construct the adapters, inject them into the use cases, and map outcomes to exit codes `0`/`1`/`2`/`3`/`4`. Return `ExitCode` — `clippy::exit` is `forbid` and no `#[allow]` can rescue it (E0453)
- [X] T060 [P] [US1] Implement human-readable output in `crates/cli/src/output.rs` reporting events seen and records landed on every run, and whether the resumption point moved
- [X] T061 [P] [US1] `crates/cli/tests/cli.rs`: assert exit code `2` on concurrent invocation (FR-010), exit `1` on an unreachable source with raw unchanged, exit `0` for a run that found nothing, and that `status` exits `0` before any run has happened

**Checkpoint**: User Story 1 fully functional. Extraction lands the full history, repeat runs land nothing, and the store refuses mutation.

---

## Phase 4: Polish & Cross-Cutting Concerns

- [ ] T062 Run `cargo sqlx prepare --workspace` and commit `.sqlx/`, then confirm the `sqlx-prepare` flake check from T013 passes
- [ ] T063 Run the full `nix flake check` and resolve every finding across the `checks` enumerated in `flake.nix` — per-crate builds, rustfmt, clippy with warnings denied, nextest, doctests, `audit-deps`, `audit-licenses`, `architecture`, `workspace-members` and `secrets`. This is the merge gate, and CI derives its jobs from it
- [ ] T064 Execute the live validation in [quickstart.md](./quickstart.md): confirm 163 `updated` ids against `workout_count`, that a second run lands nothing, and that `UPDATE`/`DELETE` are refused
- [ ] T065 [P] Run the deferred live check in [quickstart.md](./quickstart.md) with a disposable workout: land it, delete it **in the Hevy app** (the API has no `DELETE` endpoint), re-run, and confirm it lands as `deleted` while the earlier `updated` record stays intact
- [ ] T066 [P] Update `README.md` with the `fitness` commands and the configuration table
- [ ] T067 Confirm the `secrets` check in `flake.nix` passes, that `.env` remains matched by `.gitignore`, and that no credential reached version control (§ 35)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: no dependencies. T001→T002→T003→T004 are sequential; T005–T009 are parallel
- **Foundational (Phase 2)**: depends on Setup. **Blocks all story work**
- **User Story 1 (Phase 3)**: depends on Foundational
- **Polish (Phase 4)**: depends on User Story 1

### Within User Story 1

```text
Domain types (T015–T023)
        ↓
Ports declared (T024–T027)
        ↓
Integration tests written and failing (T028–T036)   ← § 31: red before green
        ↓
        ├── Store adapters (T037–T044)
        ├── Hevy adapter (T045–T052)
        └── Use case (T053–T056)   ← needs ports only, not adapters
                ↓
        CLI composition root (T057–T061)
```

The use case depends on the port *declarations*, not on the adapters — that inversion is what lets T053–T056 proceed against the fakes from T014 while the adapters are still being built.

### Critical ordering constraints

- **T024–T027 before T028–T036**: a test cannot be written against a port that does not exist. This is not a violation of tests-first — the port declaration is the contract under test, and the implementations come after
- **T037–T039 before T040**: the migration must exist before the store tests can run against it
- **T048 before T049**: provenance is read out of the parsed envelope
- **T045 is not optional and not a nice-to-have.** The empty-response key mismatch is the single highest-risk finding in the research

### Parallel Opportunities

- Setup: T005, T006, T007, T008, T009 together
- Domain vocabulary: T015, T016, T017, T018 together; then T022, T023
- Every integration test T028–T036 in parallel — separate concerns, and they are written against ports rather than implementations
- Store adapters T042, T043 in parallel after T041
- Hevy contract tests T045, T046, T047 in parallel
- Store adapters, Hevy adapter and use case are three independent tracks once the ports exist

---

## Parallel Example: User Story 1 domain vocabulary

```bash
# Launch the four value-type modules together:
Task: "Create identifier newtypes in crates/domain/src/landing/ids.rs"
Task: "Create RawPayload and PayloadDigest in crates/domain/src/landing/payload.rs"
Task: "Create EventKind sum type in crates/domain/src/landing/event.rs"
Task: "Create timestamp newtypes in crates/domain/src/landing/time.rs"

# Then the scenario tests, all against ports rather than implementations:
Task: "Scenario 1 first-run test in crates/application/tests/extraction.rs"
Task: "Scenario 5 interrupted-run test in crates/application/tests/extraction.rs"
Task: "Empty-response contract test in crates/infrastructure/tests/hevy_contract.rs"
```

---

## Implementation Strategy

### MVP

User Story 1 **is** the MVP; there is no smaller increment that delivers anything. Complete Phases 1–3, then stop and validate against the live account before polish.

1. Phase 1: Setup — workspace shape and documentation corrections
2. Phase 2: Foundational — delete the scaffolding, stand up the store
3. Phase 3: User Story 1 — types, ports, failing tests, adapters, use case, CLI
4. **STOP and VALIDATE**: run [quickstart.md](./quickstart.md) against the live account
5. Phase 4: Polish

### Checkpoints worth stopping at

- **After T014**: workspace green, no example code, fakes ready
- **After T036**: every scenario expressed as a failing test — the steering signal is in place before any adapter exists
- **After T044**: the store enforces append-only independently of the application; SC-003 is checkable with `sqlite3` alone
- **After T061**: the feature is complete and the live validation can run

---

## Notes

- `[P]` tasks touch different files and have no incomplete dependencies
- Verify each test fails before implementing against it (§ 31)
- Commit after each task or logical group; Conventional Commits, since release-please derives versions from them
- `nix flake check` is the gate. `cargo nextest run` inside `nix develop` is the fast inner loop
- **Panics are `forbid`, not `deny`.** `#[allow(clippy::unwrap_used)]` is a compile error. Fix the error handling; do not edit `Cargo.toml` to get around it
- Human sign-off before merge (§ 40). Do not merge your own work
