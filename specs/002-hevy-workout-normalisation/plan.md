# Implementation Plan: Hevy workout normalisation

**Branch**: `002-hevy-workout-normalisation` | **Date**: 2026-08-14 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-hevy-workout-normalisation/spec.md`

## Summary

Derive the normalised layer for `hevy.workouts`: read every landed record, turn
each into a gym workout in our own vocabulary, record what will not translate,
and let a deletion leave the workout it names with nothing. No canonical layer,
no analysis, no overlay.

The approach: a `fitness normalise hevy.workouts` command reads the landing
table in serve order, hands each payload to a Hevy translator behind a driven
port, and writes workouts, retractions and refusals in one transaction that
replaces the previous derivation wholesale. The translator is the only thing in
the build that knows Hevy's JSON shape; the exercise vocabulary, the entity and
every invariant sit in `domain`, which cannot see it.

Three things decide whether this is right, and all three are checkable against
the 164 records already landed:

1. **The mapping reproduces the seven.** 93 sets carry a zero load; the model
   says exactly 7 of them are errors. Which 7 falls out of the per-template
   load interpretation and nothing else, so the mapping is correct if and only
   if those 7 refuse and the other 86 translate. It is the sharpest test in the
   feature and it needs no new fixture.
2. **Refusal is a value, not a control-flow accident.** A translation returns a
   workout with its omissions listed, or a non-empty list of refusals, and § 24
   makes "no workout and no reason" unrepresentable.
3. **Retraction is absorbing, not latest-wins.** Retracted ids are collected
   across the whole derivation before any workout is emitted, so the result
   does not depend on read order (FR-028).

The spec's three questions were resolved before planning; each amended a
document. See [research.md](./research.md) for what planning added on top —
principally how a sub-kilo load gets out of JSON without ever becoming a float,
and how the exercise mapping was derived rather than guessed.

## Technical Context

**Language/Version**: Rust 1.95.0, edition 2024 (pinned in `rust-toolchain.toml`)

**Primary Dependencies**: existing — `tokio`, `serde`/`serde_json`
(`RawValue`), `sqlx` (sqlite), `jiff`, `clap`, `thiserror`. Dev: `proptest`.
**No new runtime dependency.** `jiff` already carries the IANA database this
feature needs for § II.3's zones; fixed-point load is a newtype over `i64`
rather than a decimal crate (research.md, D3)

**Storage**: SQLite, same file as extraction. One new migration,
`0002_normalisation.sql`: five tables for the entity, one for refusals, one run
log. The normalised layer is a derivation, so — unlike raw — its tables carry no
append-only trigger and a derivation replaces them wholesale

**Testing**: `cargo nextest` via `nix flake check`; integration tests at port
boundaries (§ 29) with the landed corpus as fixture; `proptest` for § 28 over
every new value type and over `NonEmpty`

**Target Platform**: Linux; deployment-agnostic (§ 34). The operator's zone is
configuration, with no default compiled in

**Project Type**: Rust workspace, hexagonal. No new crate — this feature adds
modules to all four existing rings

**Performance Goals**: None. The whole corpus is 164 payloads totalling a few
megabytes, translated in memory in one pass

**Constraints**: translation consults no overlay and no network (FR-002, FR-003);
identical output on re-derivation (FR-004); load never passes through a float
(FR-014); an unmapped template id fails the run (FR-017) while every other
defect is recorded and stepped over (FR-024)

**Scale/Scope**: 164 landing records, 163 workouts, 1,135 exercise entries,
3,779 sets, 336 supersets, 134 distinct exercise templates. Single user, single
operator (§ I)

## Constitution Check

*GATE: passed before Phase 0, re-evaluated after Phase 1 design. Re-evaluation
notes are inline; nothing changed status.*

| Rule | Status | How |
| --- | --- | --- |
| **§ II.1** Raw untouched | PASS | Derivation opens raw read-only. The append-only triggers stay; nothing in this feature writes to `hevy_workout_landing` |
| **§ II.3** One entity per record, per source | PASS | The translator takes one `RawPayload` and returns one translation. It is handed no second record and no other stream |
| **§ II.3** Retraction | PASS | A `deleted` record yields `Translation::Retraction`; the use case collects retracted ids across the whole run before emitting. New in 1.0.1 — [decision 0001](../../docs/decisions/0001-retraction-at-the-normalised-layer.md) |
| **§ II.3** Provenance mandatory | PASS | `Provenance` is a constructor argument of `GymWorkout`, not a setter. A workout without it does not compile |
| **§ II.3** Units canonicalised | PASS | `Kg`, `Metres`, `Duration` are the only carriers; the source serves kg and metres already, so this is a type change rather than a conversion |
| **§ II.3** Timestamps carry an IANA zone | PASS | `WorkoutStart` holds an instant plus a `TimeZone`; there is no constructor taking an instant alone. Zone from configuration (FR-020) |
| **§ 6** Comparability class | N/A | No metric is derived here. Load, reps, duration and distance are all source-independent, which is why the layer may hold them at all |
| **§ 7** Re-derivation from raw, no refetch | PASS | Derivation reads the landing table and nothing else. SC-004 asserts the discard-and-rebuild |
| **§ 8** Entity identity is ours | PASS | The exercise vocabulary is a `domain` enum; the Hevy template id reaches it only through the mapping, which is many-to-one and lives in the adapter |
| **§ 9** Deterministic translation is code, overlay is data | PASS | The mapping is a `match` in `infrastructure`, not a table in the database. No port the translator holds can reach an overlay, because none exists to hold |
| **§ 10** Supersession not resolved here | PASS | Two `updated` records for one id produce two workouts, both stored, neither marked. Retraction is not supersession — § 10's own sentence now says so |
| **§ 11** Prescribed never satisfies a performed query | PASS | `routine_id` is not read. `rest_after` is `None` for every set and is not reconstructed |
| **§ 15/16** Hexagonal, external systems behind ports | PASS | `LandingRecordReader`, `WorkoutTranslator`, `NormalisedWorkoutStore`, `RefusalStore`, `NormalisationRunLog`. No `serde_json` or `sqlx` type in a port signature |
| **§ 17** Deterministic first | PASS | No LLM. The mapping is authored, version-controlled code |
| **§ 19** Frontend holds no domain logic | N/A | No frontend this feature |
| **§ 20/21** Rust only | PASS | SQL confined to the store adapter; Hevy's JSON confined to the translator |
| **§ 23** No raw types at domain boundaries | PASS | `Kg`, `SignedKg`, `Metres`, `RepCount`, `Rir`, `SetKind`, `Exercise`, `NonEmpty<T>`. No `f64`, no bare `String` ([data-model.md](./data-model.md)) |
| **§ 24** Illegal states unrepresentable | PASS | Doing the most work in the feature: `Superset` cannot hold one member, `PerformedExercise` cannot hold zero sets, `Set<M>`'s measure is fixed by its exercise's vocabulary, `Load::Absolute` cannot be zero, `Translation` cannot be empty-and-unexplained |
| **§ 25** Types document | PASS | `TimedDistance`, `Rir::TwoOrThree`, `Refusal` need no gloss. `Kg` is the one that nearly failed — see Complexity Tracking |
| **§ 26** Errors typed, no panics | PASS | `thiserror`; `NormalisationError` for the run, `Refusal` for data. Panics are `forbid`, so the fixture builders return `Result` and tests unwrap at the call site |
| **§ 27** Types first | PASS | Task order is domain vocabulary → ports → translator → store → CLI |
| **§ 28** A random instance is valid | PASS | `proptest` over every newtype and over `NonEmpty`; an arbitrary `PerformedExercise` is a valid one, which is what makes the measure partition worth having |
| **§ 29/30** Integration tests at ports are primary | PASS | Every scenario in [quickstart.md](./quickstart.md) drives the use case through its ports against the landed corpus |
| **§ 31** Red-green-refactor at the port boundary | PASS | The corpus assertions (SC-001 to SC-003) are written before the mapping and fail loudly while it is incomplete — which is exactly how the mapping gets authored |
| **§ 32/33** Minimal scope, no proof-of-concept code | PASS | No canonical layer, no Session, no metric. `web` stays dormant, as recorded in 001 |
| **§ 34** Deployment-agnostic | PASS | Database path and timezone both configuration; no default zone compiled in |
| **§ 35** Credentials never in version control | N/A | This feature makes no request and holds no credential |
| **§ 36** A source being unavailable degrades | PASS | Stronger here: derivation never contacts a source at all, so it works with every source down |
| **§ 37** Partial data recorded as partial | PASS | The whole of user story 2. `rest_after` and `intensity` stay absent rather than defaulted; a refused set is omitted and named |
| **§ 38** Staleness observable | PASS | `normalisation_run` mirrors `extraction_run`; `fitness status` gains the derivation's standing beside the extraction's |
| **§ 40** Human sign-off before merge | PASS | Branch and PR; not self-merged |

### Conflicts surfaced (Governance)

**One, settled by the operator during specification, and it amended the
constitution.** § II.3's per-record rule read literally as forbidding a deletion
from affecting anything. The reading is wrong — the rule is about composition,
and a retraction composes nothing — and the constitution now says so. Version
1.0.0 → 1.0.1, PATCH.
[Decision 0001](../../docs/decisions/0001-retraction-at-the-normalised-layer.md).

**Two more settled by revising the model of record rather than the
constitution**: the distance split
([0002](../../docs/decisions/0002-distance-and-distance-over-time-are-different-measures.md))
and unrecorded resistance
([0003](../../docs/decisions/0003-unrecorded-resistance-translates-as-relative-zero.md)).

Planning raised no further conflict.

## Project Structure

### Documentation (this feature)

```text
specs/002-hevy-workout-normalisation/
├── plan.md              # This file
├── spec.md
├── research.md          # Phase 0 — mapping method, fixed point, zone handling
├── data-model.md        # Phase 1 — the entity, and its projection into SQL
├── quickstart.md        # Phase 1 — every scenario, as a runnable check
├── contracts/
│   ├── ports.md         # The five driven ports and two driving ports
│   ├── cli.md           # normalise | refusals, and status's new section
│   └── exercise-mapping.md   # How the 134 templates were resolved, and the rules
└── tasks.md             # Phase 2 — /speckit-tasks, not created here
```

### Source Code (repository root)

```text
crates/
├── domain/                 ring 0
│   └── src/
│       ├── landing/        unchanged
│       └── gym/            NEW — the entity and its vocabulary
│           ├── nonempty.rs     NonEmpty<T>
│           ├── load.rs         Kg, SignedKg, Load
│           ├── measure.rs      RepCount, Duration, Metres, Distance, TimedDistance
│           ├── intensity.rs    Rir
│           ├── exercise.rs     the four vocabularies, and Exercise over them
│           ├── set.rs          Set<M>, SetKind
│           ├── workout.rs      PerformedExercise, Superset, WorkoutItem, GymWorkout
│           └── time.rs         WorkoutStart — instant plus IANA zone
├── application/            ring 1
│   └── src/
│       ├── ports.rs        + five driven, two driving
│       ├── error.rs        + NormalisationError
│       └── normalise.rs    NEW — the use case
├── infrastructure/         ring 2
│   └── src/
│       ├── hevy/
│       │   ├── payload.rs      NEW — the serde shapes of a workout payload
│       │   ├── translate.rs    NEW — WorkoutTranslator for Hevy
│       │   └── mapping.rs      NEW — 134 template ids -> exercise + load
│       └── store/
│           ├── landing.rs      + LandingRecordReader
│           ├── normalised.rs   NEW — NormalisedWorkoutStore
│           └── refusals.rs     NEW — RefusalStore
└── cli/                    ring 3
    └── src/
        ├── catalogue.rs    unchanged — one entry per stream still
        ├── wiring.rs       + the normalisation arm
        └── main.rs         + normalise, refusals

migrations/
└── 0002_normalisation.sql  NEW
.sqlx/                      regenerated — `cargo sqlx prepare --workspace`
```

**Structure Decision**: no new crate. Extraction proved the four rings, and
normalisation is the same shape one layer up — a use case in `application`,
adapters in `infrastructure`, entry points in `cli`. The three-edit crate
process does not apply, and `crateRings` is untouched.

The one placement worth stating: **the exercise mapping lives in
`infrastructure`, not `domain`.** It is keyed on `exercise_template_id`, which
is a Hevy identifier, and a domain that knows Hevy's identifiers is a domain
shaped by a source (§ II.3). What `domain` owns is the vocabulary the mapping
points *at*. The direction is the whole of § 8: the source is translated into
our entity, never the reverse.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
| --- | --- | --- |
| `Kg` wraps `i64` milligrams (§ 25: a type needing a comment is the wrong type) | FR-014 forbids float, and the corpus holds `.1`, `.2` and `.4` hand-converted from pound machines. A fixed-point integer is the only carrier that persists, digests and compares identically across builds | `f64` fails FR-014 outright. A decimal crate adds a dependency for one field and still has to get the value out of JSON without a float on the way — which is the actual problem, and is solved by reading the number's original bytes (research.md, D3). The name is mitigated by construction: `Kg` is reachable only through `TryFrom<&str>` and `Display`, both of which speak kilograms, so no caller ever handles the integer |
| ~129 exercise variants across four enums | The vocabulary is § 8's "declared, version-controlled and owned here", and 134 templates need somewhere to land. It is large because the corpus is, not because the design is | A string-keyed exercise fails § 23 and makes a typo a new exercise. A generated enum was rejected: the mapping's judgements — which zeros are bodyweight, which category to override — are exactly what a generator cannot make, and the seven-zero test only means something if a person made them |

One further cost, accepted rather than a violation:

- **The measure partition costs a fourth `PerformedExercise` variant and four
  vocabularies.** Every function over a performed exercise now matches four
  ways. That is the price of a set and its exercise being unable to disagree
  (§ 24), and it is what makes SC-011 a compile-time fact rather than a test.

## Post-Design Constitution Re-check

Re-run after Phase 1. No gate changed status. Three observations from designing
the artifacts:

- **§ 24 is load-bearing in a way it was not in 001.** There, it stopped a run
  outcome being half-recorded. Here it is doing domain work: `Superset` holding
  `NonEmpty2<PerformedExercise>` means the two malformed groupings in the
  corpus cannot be represented, so refusing them is not a check to remember but
  the only thing the code can do. The same goes for the empty exercise entry and
  the zero-load absolute set.
- **The seven-zero test is the mapping's specification.** It was written into
  quickstart.md before the mapping exists, and it fails until every one of 134
  load interpretations is right. § 31 asks for red-green at the port boundary;
  this is the rare case where one assertion covers a hundred judgements.
- **Refusal needed a locus type, not a string.** FR-022 wants what, where and
  why, actionable without re-reading the payload. A formatted message satisfies
  a human and nothing else; `RefusalLocus` — record, exercise index, set index
  — is what makes SC-002 a query rather than a grep. Added during design.

The gap design did not close is that **supersession is still untested against
real data**, exactly as 001 left it. The corpus has 164 distinct ids and no
re-serve, so US1 scenario 7 is exercised synthetically. Recorded rather than
argued away.
