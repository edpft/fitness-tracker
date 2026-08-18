# Tasks: Prescribed workout generation

**Branch**: `003-prescribed-workout-generation` | **Plan**: [plan.md](./plan.md) | **Spec**: [spec.md](./spec.md)

**Tests are not optional here.** § 29 makes integration tests at port boundaries
the primary suite and § 31 requires red-green-refactor at that boundary, so each
story's tests are written before the code that satisfies them. Task order within
a story reflects that: the assertions come first and fail loudly.

Tests return `()` and assert by panicking (`panic_in_result_fn` is forbidden);
fixture builders are free functions returning `Result`, unwrapped at the call
site, because the test exemptions do not reach them. `#[tokio::test]` cannot be
used — build the runtime by hand from a `#[test]` returning `()`.

**Paths** are repository-relative and exact.

---

## One deviation from the spec's story boundaries, stated up front

**User Story 3's *plan* half is delivered in Phase 2, not Phase 5.**

The spec splits US3 into a plan (scenarios 1–4) and a failure mechanism
(scenarios 5–10). US1 cannot prescribe a primary slot without the plan — the top
set *is* a ladder position — so `Ladder` is a blocking prerequisite rather than a
P3 increment. Phase 5 therefore covers only the failure mechanism: hold, reset,
re-climb, resume.

This does not change what US3 delivers, and it does not make US1 depend on US3's
phase. It is recorded because silently reorganising a spec's stories is how a
plan and its tasks drift apart.

**One task is blocked, marked ⛔.** Research D8's ladder span is not yet authored,
so no real prescription can be issued. Nothing else waits on it: an earlier version
of this file also blocked the corpus comparison, on the assumption that it asserted
reproduction. It asserts attribution instead, which needs no span.

---

## Phase 1: Setup

- [X] T001 [P] Create `migrations/0003_failed_attempt.sql` adding the `outcome` discriminator to `performed_set` as create-new/copy/drop/rename, with the two `CHECK` constraints in [data-model.md](./data-model.md) § 1
- [X] T002 [P] Create `migrations/0004_prescription.sql` with the authored tables and the prescribed tables in [data-model.md](./data-model.md) §§ 2–3 — no append-only triggers, no wholesale-replacement path, `CHECK` constraints mirroring the sum types
- [X] T003 [P] Add the `prescription` module to `crates/domain/src/lib.rs` and create `crates/domain/src/prescription/mod.rs` declaring its submodules
- [X] T004 Add `toml` to `[workspace.dependencies]` in `Cargo.toml` and to `crates/infrastructure/Cargo.toml` only — it must not appear in `domain`, `application` or `cli`
- [X] T005 [P] Create `crates/infrastructure/tests/fixtures/programme.toml` from [contracts/programme.md](./contracts/programme.md), leaving the ladder span as `TODO`. The `fixtures` directory is already named in `flake.nix`'s fileset; a `.toml` fixture placed anywhere else is empty inside the nix sandbox

---

## Phase 2: Foundational

**Blocking.** Every user story needs the prescribed vocabulary, the ladder and the
ports. Nothing in Phase 3+ starts until this phase is done.

### The performed-side change (§ 27: types first)

- [X] T006 Implement `Performed<M>` with `Completed(M)` and `Failed` in `crates/domain/src/gym/outcome.rs` and re-export from `crates/domain/src/gym/mod.rs`
- [X] T007 Change `Set<M>` in `crates/domain/src/gym/set.rs` to hold `outcome: Performed<M>` in place of `measure: M`, and update `Display`
- [X] T008 Update every consumer the compiler names — `crates/domain/src/gym/workout.rs`, `crates/infrastructure/src/hevy/translate.rs`, `crates/infrastructure/src/store/normalised.rs` — mapping each existing case to `Completed(m)`. **No behaviour change yet**: the zero-reps arm is US2's work
- [X] T009 Extend `crates/infrastructure/src/store/normalised.rs` to write and read the `outcome` column against migration `0003`

### The prescribed vocabulary

- [X] T010 [P] Implement `Percentage` over integer basis points in `crates/domain/src/prescription/parameters.rs`, parsed from a string with a `%` suffix, with no float on any path
- [X] T011 [P] Implement `PlateIncrement` and `TopSetReps` in `crates/domain/src/prescription/parameters.rs`
- [X] T012 Implement `quantise(load, increment)` in `crates/domain/src/prescription/quantise.rs` — nearest multiple, exact tie resolving down, integer arithmetic throughout (D5)
- [X] T013 [P] Implement `Target<M>` in `crates/domain/src/prescription/target.rs`, with `Range` rejecting `low >= high` at construction
- [X] T014 Implement `Prescribed<M>` and `PrescribedSet<M>` in `crates/domain/src/prescription/target.rs`, every variant pinning at least one axis so "prescribes nothing" is unconstructible (FR-003)
- [X] T015 [P] Implement `SlotId` and the eleven slot constants in `crates/domain/src/prescription/shape.rs`, per the table in [contracts/programme.md](./contracts/programme.md)
- [X] T016 Implement `PrescribedExercise`, `PrescribedSuperset`, `PrescribedItem` and `WorkoutShape` in `crates/domain/src/prescription/shape.rs`, reusing the measure partition and `NonEmpty`/`AtLeastTwo` from `gym`
- [X] T017 [P] Implement `Anchor` and `AnchorProvenance` in `crates/domain/src/prescription/anchor.rs`
- [X] T018 [P] Implement `SessionRole`, `WeekIndex`, `WeekKind` and `PerRole<T>` in `crates/domain/src/prescription/schedule.rs` — `PerRole` a struct with a field per role, never a map (§ 24)
- [X] T019 Implement the cycle calendar in `crates/domain/src/prescription/schedule.rs`: a date plus a start date and weekday mapping resolves to a `WeekKind` and a `SessionRole`, resolving through the operator's zone via `jiff` and never by adding multiples of 24 hours (§ II.3, D7)

### The plan (US3's plan half — blocking for US1)

- [X] T020 Implement `Ladder` in `crates/domain/src/prescription/ladder.rs`: `start`, `end` and `climbing_weeks`, with `heavy_top_set(anchor, week)` deriving the step as `(end − start) / (climbing_weeks − 1)`, quantising through T012, returning `None` for the test week, and handling a single climbing week without dividing by zero (D2)
- [X] T021 Implement `GenerationParameters`, `WarmupStep` and `ResetProtocol` in `crates/domain/src/prescription/parameters.rs`, including `ladder_start`, `ladder_end` and `light_of_heavy`, and no `anchor_per_week`
- [X] T022 Implement the `linear` template in `crates/domain/src/prescription/linear/template.rs` — `PrimaryPattern`, `StrengthBlock` with four named pattern fields, `HypertrophyBlock` with `arms`, `forearms` and a single unsupersetted `core`, and `SlotFills` total over the eleven slots
- [X] T023 Implement `Programme` in `crates/domain/src/prescription/linear/programme.rs` carrying `anchor`, `duration_weeks`, `gating_role`, `start`, `weekdays` and `fills`, plus the three consistency checks in [contracts/ports.md](./contracts/ports.md)
- [X] T024 Implement `PrescribedWorkout` in `crates/domain/src/prescription/workout.rs` as a `WorkoutShape` plus `issued_for`, `session_role`, `week`, `anchor`, `parameters`, `programme` and `issued_at` — constructible only with all of them, so a projection cannot produce one (FR-034, D9)

### Property tests (§ 28)

- [X] T025 [P] `proptest` in `crates/domain/tests/prescription_value_types.rs`: an arbitrary instance of every type in T010–T018 is valid; `Percentage` round-trips through `Display`/`FromStr` exactly; `Target::Range` never holds `low >= high`
- [X] T026 [P] `proptest` in `crates/domain/tests/quantise.rs`: `quantise` always returns a multiple of the increment, always within half an increment of its input, and always the lower of two equidistant candidates. Pin 68 → 67.5, 78.75 → 77.5, 74.375 → 75 and 72.25 → 72.5 as named cases
- [X] T027 [P] `proptest` in `crates/domain/tests/prescription_entity.rs`: an arbitrary `Prescribed<M>` pins at least one axis, and an arbitrary `WorkoutShape` has no empty exercise and no single-member superset
- [X] T028 [P] Table and property tests for the ladder in `crates/domain/tests/ladder.rs` — US3-1 to US3-4: two inputs generate every week's loading with the last week a test; the anchor is identical in every week of a block; the step is the span over the climbing weeks, so changing the duration changes the step and not the endpoint; and a single-climbing-week ladder does not divide by zero

### Ports and errors

- [X] T029 Add `PrescriptionError` to `crates/application/src/error.rs` with `NoProgramme`, `NoParameters`, `NotAProgrammedDay`, `InconsistentProgramme` and `Store`, and no variant for an underivable slot
- [X] T030 Declare the five driven ports in `crates/application/src/ports.rs` — `ExerciseHistory` (both reads plus `newest_performance`), `PerformedWorkoutReader`, `GenerationParameterStore`, `ProgrammeStore`, `PrescribedWorkoutStore` — plus `LastPerformance`, `Performance`, `PerformedSetSummary` and `UnderivableSlot`
- [X] T031 Declare the driving ports `WorkoutPrescriber` and `ProgrammeAuthor` in `crates/application/src/ports.rs`, with `Prescription` carrying `freshly_issued`, `history_through` and `underivable`, and re-export the flat surface from `crates/application/src/lib.rs`
- [X] T032 Extend `crates/infrastructure/tests/fixtures/` support with builders for a `Programme`, a `GenerationParameters` and a synthetic performed history, each returning `Result` and unwrapped at the call site

**Checkpoint**: the domain compiles, property tests pass, the ladder generates a
block from two inputs, and the ports exist with no implementation.

---

## Phase 3: User Story 1 — Issue the next prescribed workout (P1) 🎯 MVP

**Goal**: a complete, trainable prescription for a named date, with no arithmetic
done by hand.

**Independent test**: with a programme authored, `fitness prescribe --date
2026-08-17` prints five blocks in fatigue order, the strength block's four
patterns with the upper pair supersetted, the hypertrophy block's two supersets
and single core slot, the primary's loads from the anchor and ladder, and every
other slot's from its own last performance.

### Tests first (§ 31)

- [X] T033 [P] [US1] Write quickstart group 1 scenarios US1-1 to US1-3 and US1-5 in `crates/infrastructure/tests/prescription.rs` — structure, fatigue order, the primary reading no performed value, and the back-off following the top set
- [X] T034 [P] [US1] Write US1-6 in `crates/infrastructure/tests/prescription.rs` — the alternating hip-dominant fill reads two sessions back. Assert on the **date** the history came from, not only the load
- [X] T035 [P] [US1] Write US1-7 in `crates/infrastructure/tests/prescription.rs` — a never-performed exercise is reported as underivable, with no guessed load
- [X] T036 [P] [US1] Write US1-8 and US1-9 in `crates/infrastructure/tests/prescription.rs` — the prescription is stored in full, and no performed query returns prescribed data
- [X] T037 [P] [US1] Write the § 10 supersession test in `crates/infrastructure/tests/prescription.rs` — two landing records sharing a source id, synthetic, where the later-served one is the history read (D3)
- [X] T038 [P] [US1] Write the SC-008 test in `crates/infrastructure/tests/prescription.rs` — discard all generated output, regenerate from stored authored data, assert identical

### The read side

- [X] T039 [US1] Implement `ExerciseHistory` in `crates/infrastructure/src/store/history.rs`: `last_performances` batched over the exercises asked about, `performances` for one exercise oldest-first, `newest_performance`, all excluding warm-ups and all resolving § 10 by `serve_ordinal` in the `WHERE` clause
- [X] T040 [P] [US1] Implement `PerformedWorkoutReader` in `crates/infrastructure/src/store/performed.rs`, returning whole `GymWorkout`s in a date range with § 10 applied

### The authored side

- [X] T041 [P] [US1] Implement the TOML shapes and their conversion into `domain` types in `crates/infrastructure/src/programme/document.rs`, rejecting any remaining `TODO` by field path. No `toml` type escapes this module
- [X] T042 [P] [US1] Implement `GenerationParameterStore` in `crates/infrastructure/src/store/parameters.rs`, superseding by `authored_at` and never overwriting (§ 12)
- [X] T043 [P] [US1] Implement `ProgrammeStore` in `crates/infrastructure/src/store/programme.rs`, same supersession rule
- [X] T044 [US1] Implement `PrescribedWorkoutStore` in `crates/infrastructure/src/store/prescription.rs` — `issue` writing once and never rewriting, `issued_for` reading back a date

### Generation

- [X] T045 [US1] Implement the generation use case in `crates/application/src/prescribe.rs`: resolve the date to a week and role, read the ladder position, derive the primary's warm-ups, top set and back-offs, derive every other slot by double progression from `last_performances`, and collect `UnderivableSlot`s as values rather than errors (FR-011)
- [X] T046 [US1] Implement `ProgrammeAuthor` in `crates/application/src/prescribe.rs`, running the three consistency checks before storing
- [X] T047 [US1] Make `prescribe` idempotent per date: read `issued_for` first and return what was already issued with `freshly_issued: false` (FR-010)

### The CLI

- [X] T048 [US1] Add `prescribe` to `crates/cli/src/main.rs` using the clap **builder** API, with `--date` defaulting forward to the next programmed day at or after today
- [X] T049 [US1] Add `programme author` and `programme show` to `crates/cli/src/main.rs`, and the prescription arm to `crates/cli/src/wiring.rs`
- [X] T050 [US1] Implement the rendering in `crates/cli/src/output.rs` — the header with date, weekday, role, week, ladder percentage, anchor with provenance and `history through`; blocks with their slots; supersets bracketed; underivable slots in place
- [X] T051 [US1] Implement the `programme show` ladder table in `crates/cli/src/output.rs` — every week with its percentage, heavy and light loads, and state
- [ ] T052 [US1] Pin the composed `--date` default in its own unit test in `crates/cli/src/config.rs`. A stub cannot catch a wrong default, and "next programmed day at or after today" is exactly the kind that passes against a mock and fails in the gym
- [X] T053 [US1] Regenerate `.sqlx` with `cargo sqlx prepare --workspace`

### The round trip (D9, SC-010)

- [ ] T054 [P] [US1] Implement `project(&GymWorkout) -> Projection` and `ProjectionGap` in `crates/domain/src/prescription/project.rs`, assigning slots by position against the template and recording `SlotUnassignable` where the structure diverges
- [ ] T055 [US1] Implement `satisfies(performed, prescribed) -> Vec<Divergence>` in `crates/domain/src/prescription/project.rs`, treating a projected `Exactly(n)` as agreeing with a prescribed `Range` containing `n`
- [ ] T056 [P] [US1] Write SC-010a, SC-010b and SC-010d in `crates/infrastructure/tests/round_trip.rs` — all fifteen sessions project; generation reproduces their structure; satisfaction is direction-aware
- [ ] T057 [P] [US1] Write SC-010e as a compile-fail test in `crates/domain/tests/`: `store.issue(projection.shape)` must not compile
- [ ] T058 [US1] Write SC-010c in `crates/infrastructure/tests/round_trip.rs` — compare generation against what was prescribed from 2026-08-07 onward, and assert every divergence is attributable to an unstated parameter, a template change, or hand arithmetic. A divergence outside those is a defect. **No longer blocked**: attribution does not need the ladder span, where reproduction would have

**Checkpoint**: a workout is issued and printed for a real date, from real
history, with everything except the primary's absolute loads verified.

---

## Phase 4: User Story 2 — A failed attempt is recorded, not refused (P2)

**Goal**: the 95kg attempt of 2026-07-03 becomes part of the training record.

**Independent test**: re-derive the normalised layer; the attempt appears as a
failed attempt against the front squat, refusals fall from three to two, and no
total moves.

### Tests first (§ 31)

- [ ] T059 [P] [US2] Write US2-1 and US2-4 in `crates/infrastructure/tests/failed_attempt.rs` — the zero-rep set becomes a failed attempt, distinguishable from an absence
- [ ] T060 [P] [US2] Write US2-2 in `crates/infrastructure/tests/failed_attempt.rs` — 77 `failure`-typed sets in the corpus, exactly one of them a failed attempt. This is the guard against keying on the set type
- [ ] T061 [P] [US2] Write US2-3 and SC-007 in `crates/infrastructure/tests/failed_attempt.rs` as a **diff**: compute every total, count and estimate before and after the change and assert equality. A hard-coded expected total would pass even if the failure were being counted
- [ ] T062 [P] [US2] Write US2-5 in `crates/infrastructure/tests/failed_attempt.rs` — re-derive twice over unchanged raw, assert identical

### Implementation

- [ ] T063 [US2] Add the zero-reps arm to `crates/infrastructure/src/hevy/translate.rs`, keyed on `reps == 0` and **not** on the `failure` set type, producing `Performed::Failed`
- [ ] T064 [US2] Remove `RefusalReason::ZeroReps` from `crates/domain/src/gym/refusal.rs` and its arm from the translator, and update the refusal-count assertions in `crates/infrastructure/tests/normalisation.rs` and `crates/infrastructure/tests/refusals.rs` from three to two

**Checkpoint**: `fitness refusals` lists two, both malformed groupings, and the
failed attempt is in the record.

---

## Phase 5: User Story 3 — Failure handling (P3)

**Goal**: when the plan turns out to have been too ambitious, the ladder drops
back and re-climbs rather than stopping.

**Independent test**: drive the eleven-week worked example from
`primary-lift-progression.md` and match it load for load, with the anchor
unchanged in every row.

**Depends on US2** — a stall is detected from a failed attempt, which does not
exist in the record until Phase 4 is done.

### Tests first (§ 31)

- [ ] T065 [P] [US3] Write `a_reset_never_touches_the_anchor` in `crates/infrastructure/tests/progression.rs` first. FR-021 is the whole point of separating the plan from the failure mechanism, and it is the invariant an implementation is most likely to break for convenience
- [ ] T066 [P] [US3] Write US3-5 to US3-8 in `crates/infrastructure/tests/progression.rs` — hold, suspend, resume at the suspended week, and the second reset
- [ ] T067 [P] [US3] Write SC-005 in `crates/infrastructure/tests/progression.rs` — the eleven-week table, load for load, anchor constant
- [ ] T068 [P] [US3] Write US3-9 and US3-10 in `crates/infrastructure/tests/progression.rs` — a test anchors the next block; a non-gating miss does not touch the ladder
- [ ] T069 [P] [US3] Write `asking_twice_does_not_double_advance` in `crates/infrastructure/tests/progression.rs` — a regression test against reintroducing stored position state

### Implementation

- [ ] T070 [US3] Implement ladder-position derivation in `crates/domain/src/prescription/ladder.rs`: walk the gating sessions in order, advancing on a completed top set, holding on a failure, and suspending on a second failure at a load already failed
- [ ] T071 [US3] Implement the reset sequence in `crates/domain/src/prescription/ladder.rs` — drop taken from the **failed load**, re-climb at the reset's rate, resume the ladder at the suspended week when the re-climb reaches that load (FR-019, FR-020, FR-021)
- [ ] T072 [US3] Wire the derivation into `crates/application/src/prescribe.rs` via `ExerciseHistory::performances`, gated to the programme's gating role
- [ ] T073 [US3] Surface the ladder state in `programme show` and `fitness status` in `crates/cli/src/output.rs`

**Checkpoint**: the worked example reproduces exactly, and the anchor has not
moved once.

---

## Phase 6: Polish

- [ ] T074 [P] Write `docs/decisions/0006-prescription-reads-the-normalised-layer.md` — the § II.4 deviation, why supersession could not be deferred with matching, and what stays unresolved
- [ ] T075 [P] Write `docs/decisions/0007-a-zero-rep-set-is-a-failed-attempt.md` — the reversal of 002's shipped behaviour, and the 77-versus-1 evidence
- [ ] T076 [P] Add the `prescription` section to `fitness status` in `crates/cli/src/output.rs` and `crates/cli/src/main.rs` (§ 38)
- [ ] T077 [P] Update `docs/gym-workout-domain-model.md` — its open question 3 is resolved by `Performed<M>`, and `Set<M>` no longer holds `measure`
- [ ] T078 Run `nix flake check` and confirm `architecture` verifies no `toml` type in `domain`, and `use-case-isolation` still passes
- [ ] T079 Walk [quickstart.md](./quickstart.md) end to end against the real store
- [ ] T080 ⛔ Author the ladder span in `crates/infrastructure/tests/fixtures/programme.toml` and in the operator's own programme, then unblock T058 and issue a real workout (SC-001)

---

## Dependencies & Execution Order

### Phase dependencies

- **Phase 1 Setup**: no dependencies
- **Phase 2 Foundational**: depends on Setup — **blocks every user story**
- **Phase 3 US1**: depends on Foundational only
- **Phase 4 US2**: depends on Foundational only — independent of US1
- **Phase 5 US3**: depends on Foundational **and US2** (a stall needs a failed attempt to detect)
- **Phase 6 Polish**: depends on the stories it touches

### Within Phase 2

T006 → T007 → T008 → T009 is a chain: the type, the field, the consumers, the
store. T010–T019 are largely parallel. T020 depends on T012 and T018. T022 → T023
→ T024.

### Within a story

Tests before implementation (§ 31). Domain before ports before adapters before
CLI (§ 27).

### Parallel opportunities

- T001, T002, T003, T005 in Phase 1
- T010, T011, T013, T015, T017, T018 in Phase 2
- All four property and table test tasks T025–T028
- Every test task in Phase 3 (T033–T038) and Phase 4 (T059–T062) and Phase 5 (T065–T069)
- T039 and T040; T041, T042 and T043
- All four Polish documentation tasks T074–T077

---

## Parallel example: Phase 3 tests

```bash
# All six US1 test tasks touch one new file plus fixtures; write them together,
# then watch them all fail before T039.
T033  structure, order, primary reads no performed value, back-off
T034  the alternating fill reaches two sessions back
T035  a never-performed slot is reported
T036  stored in full; no performed query returns prescribed data
T037  § 10 supersession, synthetic
T038  discard and regenerate identically
```

---

## Implementation strategy

### MVP: Phases 1–3

Setup, Foundational, then User Story 1. At the end of Phase 3 the operator can
run `fitness prescribe` and get a trainable session. That is the whole point of
the feature and everything after it is correctness over time.

**The MVP is reachable without the ladder span.** T058 and T080 are the only
blocked tasks in it, and both concern the primary's absolute loads. The structure,
the accessories, the storage and the round trip are all verifiable now.

### Incremental delivery

1. Phases 1–2 → the domain and the ports exist, the ladder generates a block
2. Phase 3 → **a workout comes out** — MVP
3. Phase 4 → the record stops losing a failed attempt
4. Phase 5 → the plan survives its own failure
5. Phase 6 → decisions recorded, status wired, quickstart walked

### Notes

- Commit after each task or logical group; branch is already cut, PR at the end,
  human sign-off before merge (§ 40)
- `cargo sqlx prepare --workspace` after any query change, or the build fails
  offline rather than falling back
- Panics are `forbid`: no `#[tokio::test]`, no clap derive, no `#[allow]`
