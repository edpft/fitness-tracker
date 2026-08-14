# Tasks: Hevy workout normalisation

**Branch**: `002-hevy-workout-normalisation` | **Plan**: [plan.md](./plan.md) | **Spec**: [spec.md](./spec.md)

**Tests are not optional here.** § 29 makes integration tests at port boundaries
the primary suite and § 31 requires red-green-refactor at that boundary, so each
story's tests are written before the code that satisfies them. Task order within
a story reflects that: the assertions come first and fail loudly.

Tests return `()` and assert by panicking (`panic_in_result_fn` is forbidden);
fixture builders are free functions returning `Result`, unwrapped at the call
site, because the test exemptions do not reach them.

**Paths** are repository-relative and exact.

---

## Phase 1: Setup

- [ ] T001 Move the newtype macros from `crates/domain/src/landing/newtype.rs` to `crates/domain/src/newtype.rs` and re-export, so `landing` and `gym` share one copy; update `crates/domain/src/landing/mod.rs` and every `use` in `crates/domain/src/landing/`
- [ ] T002 [P] Create `migrations/0002_normalisation.sql` with the seven tables in [data-model.md](./data-model.md), their `CHECK` constraints mirroring the sum types, and no append-only triggers (D6)
- [ ] T003 [P] Add the `gym` module to `crates/domain/src/lib.rs` and create `crates/domain/src/gym/mod.rs` declaring its submodules

---

## Phase 2: Foundational

**Blocking.** Every user story needs the entity and the ports. Nothing in Phase 3+ starts until this phase is done.

### The domain vocabulary (§ 27: types first)

- [ ] T004 [P] Implement `NonEmpty<T>` and `AtLeastTwo<T>` in `crates/domain/src/gym/nonempty.rs`, each with a fallible constructor, `first`, `iter`, `len` and `IntoIterator`
- [ ] T005 [P] Implement `Kg` and `SignedKg` over `i64` milligrams in `crates/domain/src/gym/load.rs`, constructed only from a decimal string, rejecting more than three decimal places; `Display` renders kilograms
- [ ] T006 [P] Implement `Metres`, `Duration`, `RepCount` (rejecting zero), `Distance` and `TimedDistance` in `crates/domain/src/gym/measure.rs`
- [ ] T007 [P] Implement `Rir`'s eight ordered positions in `crates/domain/src/gym/intensity.rs` with `PartialOrd`/`Ord` and no arithmetic, and no `From<u8>`
- [ ] T008 Implement `Load` and its fallible `absolute` constructor in `crates/domain/src/gym/load.rs`, rejecting zero with the reason FR-010 names
- [ ] T009 [P] Implement `SetKind` and `Set<M>` in `crates/domain/src/gym/set.rs`, with `intensity` and `rest_after` optional
- [ ] T010 Declare the four exercise vocabularies in `crates/domain/src/gym/exercise.rs` — `RepsExercise`, `DurationExercise`, `DistanceExercise`, `TimedDistanceExercise` — with a `name()` for each, populated as the mapping in T021 requires
- [ ] T011 Implement `WorkoutStart` in `crates/domain/src/gym/time.rs` over a `jiff` instant plus `TimeZone`, with no constructor taking an instant alone, and a `wall_clock()` that resolves through the zone
- [ ] T012 Implement `PerformedExercise`, `Superset`, `WorkoutItem` and `GymWorkout` in `crates/domain/src/gym/workout.rs`, reusing `domain::landing::Provenance` and keyed by `LandingRecordId`
- [ ] T013 [P] Implement `Refusal`, `RefusalLocus`, `RefusalReason` and `RefusalReason::kind()` in `crates/domain/src/gym/refusal.rs`
- [ ] T014 [P] Implement `LandingRecordId` in `crates/domain/src/landing/ids.rs` and expose it on `LandingRecord`, so a derivation can key on the record it came from (D8)

### Property tests (§ 28)

- [ ] T015 [P] `proptest` in `crates/domain/tests/gym_value_types.rs`: an arbitrary instance of every type in T004–T011 is valid, `Kg` round-trips through `Display`/`FromStr` exactly, and `Rir` orders without arithmetic
- [ ] T016 [P] `proptest` in `crates/domain/tests/gym_entity.rs`: an arbitrary `GymWorkout` is valid — no empty exercise, no single-member superset, no absolute zero load, and no set whose measure disagrees with its exercise

### Ports and errors

- [ ] T017 Add `NormalisationError` and extend `StoreError` as needed in `crates/application/src/error.rs`, with no variant for bad data ([contracts/ports.md](./contracts/ports.md))
- [ ] T018 Declare `LandingRecordReader`, `WorkoutTranslator`, `NormalisedWorkoutStore`, `RefusalStore` and `NormalisationRunLog` in `crates/application/src/ports.rs`, plus `Translation`, `OperatorZone`, `NormalisationSummary` and `RefusalReport`
- [ ] T019 Declare the driving ports `WorkoutNormaliser` and `RefusalReporter` in `crates/application/src/ports.rs` and re-export the flat surface from `crates/application/src/lib.rs`
- [ ] T020 [P] Build the corpus test fixture in `crates/application/tests/support/corpus.rs`: load `local.db`'s 164 landing records into a temporary database per test, plus builders for synthetic records

**Checkpoint**: the domain compiles, property tests pass, and the ports exist with no implementation.

---

## Phase 3: User Story 1 — Normalise the landed history (P1)

**Goal**: every landed Hevy workout becomes a gym workout in our vocabulary.

**Independent test**: derive over the 164 records; 163 workouts, 1,135 entries, 3,755 of 3,779 sets, 334 of 336 supersets, 134 of 134 templates resolved; derive again and get the identical result.

### Tests first (§ 31)

- [ ] T021 [P] [US1] Write quickstart scenario 1 in `crates/application/tests/normalisation.rs` — the corpus's entity counts, asserted against the model of record's figures
- [ ] T022 [P] [US1] Write quickstart scenario 2 in `crates/application/tests/normalisation.rs` — derive, re-derive, discard and re-derive; all three equal, and equal again with the landing records reversed
- [ ] T023 [P] [US1] Write quickstart scenario 3 in `crates/application/tests/normalisation.rs` — the pull-up and chest-dip collapses, template `DDB29047` under both titles, both `Overhead Squat` ids
- [ ] T024 [P] [US1] Write quickstart scenario 7 in `crates/application/tests/normalisation.rs` — wall clock across both switchovers, and a refusal to run with no declared zone
- [ ] T025 [P] [US1] Write quickstart scenarios 9 and 11 in `crates/application/tests/normalisation.rs` — an unmapped id stops the run and writes nothing; two records sharing a source id produce two workouts

### The translator

- [ ] T026 [US1] Define the payload's serde shapes in `crates/infrastructure/src/hevy/payload.rs`, reading the weight field as `&RawValue` so a load never passes through an `f64` (D3)
- [ ] T027 [US1] Author the 134-entry mapping in `crates/infrastructure/src/hevy/mapping.rs` — exercise, measure and load interpretation per template id, per the rules in [contracts/exercise-mapping.md](./contracts/exercise-mapping.md)
- [ ] T028 [US1] Implement `WorkoutTranslator` for Hevy in `crates/infrastructure/src/hevy/translate.rs`: items in recorded order, superset grouping with contiguity checked against that order, RPE to `Rir`, set kind, and `Relative`/`Absolute`/negated load
- [ ] T029 [P] [US1] Unit-test the mapping's totality in `crates/infrastructure/src/hevy/mapping.rs` — every id the corpus holds resolves, and no two entries disagree about one id

### The store and the use case

- [ ] T030 [P] [US1] Implement `LandingRecordReader` in `crates/infrastructure/src/store/landing.rs`, read-only over `hevy_workout_landing`, oldest first
- [ ] T031 [P] [US1] Implement `NormalisedWorkoutStore` in `crates/infrastructure/src/store/normalised.rs` — `replace` deleting and rewriting in one transaction (D6)
- [ ] T032 [P] [US1] Implement `NormalisationRunLog` in `crates/infrastructure/src/store/run_log.rs`, mirroring `ExtractionRunLog`
- [ ] T033 [US1] Implement the use case in `crates/application/src/normalise.rs`: read raw, translate each record, collect, write in one transaction, return the summary
- [ ] T034 [US1] Wire `normalise` in `crates/cli/src/wiring.rs` and `crates/cli/src/main.rs` using clap's builder API, reading `FITNESS_TIMEZONE` and failing without it (D4)
- [ ] T035 [US1] Add the `normalise` output format to `crates/cli/src/output.rs` per [contracts/cli.md](./contracts/cli.md), and a CLI test in `crates/cli/tests/cli.rs`

**Checkpoint**: `fitness normalise hevy.workouts` derives 163 workouts and reproduces every count in SC-001 and SC-003.

---

## Phase 4: User Story 2 — Refusal is recorded, never guessed (P2)

**Goal**: what will not translate is visible, diagnosable and distinguishable by kind.

**Independent test**: the refusals are exactly the named 24 sets and 2 supersets, each naming the record, the position and a reason.

### Tests first

- [ ] T036 [P] [US2] Write quickstart scenario 4 in `crates/application/tests/refusals.rs` — exactly 7 zeros refuse, on the four templates named, and the other 86 translate as `Relative`. **The mapping's specification**
- [ ] T037 [P] [US2] Write quickstart scenario 5 in `crates/application/tests/refusals.rs` — the refusal set is exactly the named one, asserted over `RefusalReason` rather than rendered text, with the two malformed groupings named and their members still translating
- [ ] T038 [P] [US2] Write quickstart scenario 6 in `crates/application/tests/refusals.rs` — 2,415 intensities present and 1,364 absent, `rest_after` absent throughout, no title or note carried
- [ ] T039 [P] [US2] Write quickstart scenario 10 in `crates/application/tests/refusals.rs` — every record accounted for exactly once, and the summary's numbers reconcile

### Implementation

- [ ] T040 [US2] Implement `RefusalStore` in `crates/infrastructure/src/store/refusals.rs`, replacing per run and reading back for FR-023
- [ ] T041 [US2] Record refusals from the use case in `crates/application/src/normalise.rs`, in the same transaction as the workouts
- [ ] T042 [US2] Implement `RefusalReporter` in `crates/application/src/normalise.rs`, grouping by `RefusalReason::kind()`
- [ ] T043 [US2] Wire `fitness refusals` in `crates/cli/src/wiring.rs` and `crates/cli/src/main.rs`, with the grouped output in `crates/cli/src/output.rs` per [contracts/cli.md](./contracts/cli.md)
- [ ] T044 [P] [US2] Add a CLI test in `crates/cli/tests/cli.rs` — `refusals` exits `0` with refusals present, and `normalise` exits `0` having recorded 26

**Checkpoint**: `fitness refusals hevy.workouts` lists 26, grouped into wrong data, declared limitation and unmodelled.

---

## Phase 5: User Story 3 — A withdrawn workout is absent (P3)

**Goal**: a deletion leaves the workout it names with no normalised entity, order-independently.

**Independent test**: the corpus's `deleted` record withdraws nothing and refuses nothing; a synthetic pair yields 163 workouts in either landing order.

### Tests first

- [ ] T045 [P] [US3] Write quickstart scenario 8 in `crates/application/tests/retraction.rs` — the corpus's tombstone, the synthetic pair in both orders, and every other workout unchanged
- [ ] T046 [P] [US3] Write the retraction-is-not-a-refusal assertion in `crates/application/tests/retraction.rs` — the `deleted` record appears in no refusal (FR-027)

### Implementation

- [ ] T047 [US3] Translate a `deleted` payload to `Translation::Retraction` in `crates/infrastructure/src/hevy/translate.rs`, handling the body-less shape `{type, id, deleted_at}`
- [ ] T048 [US3] Apply retraction absorbingly in `crates/application/src/normalise.rs` — collect retracted ids across every record before emitting any workout (D5, FR-028)
- [ ] T049 [US3] Report `retractions_applied` in `NormalisationSummary` and in the `normalise` output in `crates/cli/src/output.rs`

**Checkpoint**: all three stories complete; SC-001 through SC-011 hold.

---

## Phase 6: Polish

- [ ] T050 [P] Extend `fitness status` in `crates/application/src/status.rs`, `crates/cli/src/output.rs` and `crates/cli/src/main.rs` with the normalisation section and `records behind` (§ 38)
- [ ] T051 [P] Write quickstart scenario 12 in `crates/application/tests/normalisation.rs` — derivation and extraction do not contend
- [ ] T052 Regenerate `.sqlx` with `cargo sqlx prepare --workspace`; a stale directory is a compile error, not a fallback
- [ ] T053 [P] Update `CLAUDE.md`'s layout and conventions with what this feature established — the mapping's placement, and that `application::normalise::` joins `extract` and `status` behind the use-case-isolation check
- [ ] T054 [P] Update `README.md` with the two new commands
- [ ] T055 Run `nix flake check` and fix what it finds

---

## Dependencies

```text
Phase 1 Setup
      ↓
Phase 2 Foundational  ← blocks everything
      ↓
Phase 3 US1 (P1)  ← the MVP; delivers the layer
      ↓
Phase 4 US2 (P2)  ← needs US1's translator to have something to refuse
      ↓
Phase 5 US3 (P3)  ← independent of US2; needs US1's use case
      ↓
Phase 6 Polish
```

US2 and US3 are independent of each other and can be built in either order once
US1 is done. Neither is worth building against an empty normalised layer, which
is why both sit behind US1 rather than beside it.

## Parallel opportunities

- **Phase 2**: T004–T007, T009, T013, T014 are seven separate files with no
  dependency between them. T015 and T016 follow the types they cover.
- **Phase 3**: T021–T025 are five test files written together, then T030–T032
  are three store adapters in three files.
- **Phase 4**: T036–T039 together.
- **Phase 6**: everything except T052 and T055, which come last and in that order.

The serial spine is T027 → T028 → T033 → T034. The mapping is the long pole and
it is one file, so it does not parallelise.

## Implementation strategy

**MVP is Phase 3.** US1 alone delivers the normalised layer and everything the
canonical layer will need to read. It reproduces SC-001 and SC-003, which is the
feature's reason to exist.

US2 makes the 24 refusals diagnosable rather than merely counted; US3 handles
one record in 164. Both are real obligations — § 37 and "nothing is skipped
silently" — and neither changes what US1 produces for the other 163.

Ship order is priority order, and each checkpoint is a working `fitness`
command.
