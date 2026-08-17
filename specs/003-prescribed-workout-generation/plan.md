# Implementation Plan: Prescribed workout generation

**Branch**: `003-prescribed-workout-generation` | **Date**: 2026-08-17 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/003-prescribed-workout-generation/spec.md`

## Summary

Issue the next prescribed workout for a named date: build it from the authored
programme, derive the primary's loads from an anchor, derive every other slot's
from the most recent performance of the exercise being prescribed, store what was
issued, and print it. Plus the one performed-side change the gate depends on — a
zero-rep set becomes a failed attempt instead of a refusal.

The approach: a `fitness prescribe --date` command reads the authored programme
and parameters, projects each slot's exercise history out of the normalised
layer, hands both to a generation use case in `application`, and writes the
issued prescription. The template and the progression rule are `domain`; the
authored inputs are data behind a store port; nothing in the generation path
knows Hevy exists.

Five decisions carry the design, and three of them are checkable against the 164
workouts already normalised:

1. **The programme is a linear block: two inputs in, a ladder out.** Given a
   duration in weeks and a starting 1RM, generate the whole primary loading series
   — percentages of a **fixed** anchor climbing to an authored endpoint, with the
   last week a test. The endpoint is authored and the weekly step derived from it
   and the duration, because an endpoint is a claim about achievable gain while a
   step is a number with nothing behind it.

   **Revised during planning.** The first version had the anchor itself climbing
   +2.5kg per week — the same load sequence described from the wrong end, leaving
   the block no endpoint and nothing for a duration to be the duration *of*. The
   ladder *position* is still derived, so FR-010 and § 7 hold exactly as before.
   [research.md](./research.md), D2.

   **The plan and the failure mechanism are separate**, and conflating them was the
   wrong turn. A stall suspends the ladder, drops from the *failed load*, re-climbs
   at the reset's rate, and resumes where it left off. It never touches the anchor:
   a stall is evidence the plan was too ambitious, not evidence about where the
   block started.
2. **A failed attempt is a set outcome, not a set kind and not a zero count.**
   `Set<M>` stops holding `measure: M` and holds `outcome: Performed<M>`, whose
   variants are `Completed(M)` and `Failed`. `RepCount` stays non-zero, so a
   failure cannot reach a volume sum by arithmetic rather than by discipline
   (FR-029). D1.
3. **The generation query resolves supersession as it reads.** The spec defers
   the canonical layer, but § 10's within-source rule is not deferrable with it:
   two landing records for one workout are both normalised, and a projection
   reading both would prescribe from a performance the source has withdrawn. The
   projection takes the latest-served record per source id — one `WHERE` clause,
   deterministic, consulting no overlay. No such pair exists in the corpus today,
   which is why this is cheap now and expensive to find later. D3, and surfaced
   as a deviation below.
4. **Quantisation is one function, applied everywhere a derived load meets the
   grid.** Nearest, ties down — settled in the spec. It governs back-offs,
   warm-up steps and reset drops alike, so it is a `domain` function over a load
   and an increment rather than a rule attached to the back-off. D5.
5. **A workout's shape is separable from its issuance.** Raised by the operator
   during planning: a performed workout can be projected into prescription's
   vocabulary, so the two share a structure and a round trip exists. `WorkoutShape`
   is that structure; `PrescribedWorkout` is a shape plus the anchor, cycle,
   parameters and date that make it issued. Only generation can build the second,
   so a projection cannot be stored as a prescription — which is the § 11 hazard the
   split closes. D9.

Two of the spec's success criteria are the sharp tests and neither needs a new
fixture: **SC-003**, that regenerating the sessions from 2026-08-07 onward
reproduces the loads actually prescribed, and **SC-006/SC-007**, that the 95kg
attempt becomes visible without moving any total. D9 changes how the first is
checked — SC-010 makes it a comparison of two values rather than a reading of
printed output.

## Technical Context

**Language/Version**: Rust 1.95.0, edition 2024 (pinned in `rust-toolchain.toml`)

**Primary Dependencies**: existing — `tokio`, `serde`, `sqlx` (sqlite), `jiff`,
`clap`, `thiserror`. Dev: `proptest`. **One new runtime dependency**: `toml`, for
the authored programme document (Complexity Tracking). `jiff` already supplies
the calendar arithmetic the cycle schedule needs, which must resolve through the
operator's zone rather than assume a 24-hour day (§ II.3)

**Storage**: SQLite, same file. Two new migrations. `0003_failed_attempt.sql`
adds the set outcome discriminator to `performed_set`; `0004_prescription.sql`
adds the authored tables — generation parameters, programme, and issued
prescribed workouts. The authored tables are § III data: they carry no raw layer
and, unlike the normalised layer, are **not** replaced wholesale, because nothing
regenerates them if lost (§ 12)

**Testing**: `cargo nextest` via `nix flake check`; integration tests at port
boundaries (§ 29). Generation suites need the Hevy translator to build history
from the landed corpus, so they live in `crates/infrastructure/tests/` — the same
placement, and for the same reason, as the normalisation suites. `proptest` for
§ 28 over every new value type

**Target Platform**: Linux; deployment-agnostic (§ 34). The operator's zone stays
configuration with no default compiled in

**Project Type**: Rust workspace, hexagonal. No new crate — modules added to all
four existing rings

**Performance Goals**: None. Generation reads 164 workouts and writes one
prescription

**Constraints**: generation reads the performed layer and never writes it
(§ 11); the primary's loads read no performed value except completion or failure
of the gating top set (FR-013); no load passes through a float (inherited from
002); prescribed data satisfies no performed query (FR-027); regeneration from
stored authored data is identical (SC-008)

**Scale/Scope**: 164 normalised workouts, 1,122 exercise entries, 3,755 sets. One
programme, one primary lift, one template, 11 slots. Single user, single operator
(§ I)

**NEEDS CLARIFICATION — not a design unknown, an authored value.** The ladder's
start and end percentages are an operator input this plan cannot derive: they are a
claim about achievable gain, not anything the record implies. Everything else is now
evidenced — the anchor is the 3 July test at 90kg, the rep counts are constant per
role, and the light-of-heavy and back-off percentages reproduce the record. The span
does not block Phase 1 design; it does block SC-001 and SC-003. See
[research.md](./research.md), D8.

## Constitution Check

*GATE: passed before Phase 0, re-evaluated after Phase 1 design. One conflict is
surfaced rather than resolved silently; see below.*

| Rule | Status | How |
| --- | --- | --- |
| **§ II** scope — this is not observation data | PASS | The programme, its parameters and every issued prescription are § III data. They acquire no raw, normalised or canonical layer, and no table in `0004` has an append-only trigger or a rebuild path |
| **§ II.1** Raw untouched | PASS | Generation opens raw not at all. The failed-attempt change reads raw and writes only the normalised layer |
| **§ II.3** Per-record, per-source | PASS | The failed-attempt change alters what one record translates to, not how many records an entity is built from |
| **§ II.4** Canonical is the layer analysis reads | **DEVIATION** | No canonical layer is built; the history projection reads normalised. Operator-decided, risk stated, and § 10's within-source supersession is applied in the read rather than skipped. See "Conflicts surfaced" |
| **§ II.5** Analytical is not a system of record | PASS | The history projection stores nothing. It is a query, re-run on each generation |
| **§ 6** Comparability class | PASS | Load and reps are source-independent, which is what lets a history projection combine them at all. No method-dependent value is read |
| **§ 7** Re-derivation from inputs | PASS | Given the authored programme, the parameters and raw, every prescription this feature has ever issued re-derives. The anchor is authored and fixed; the ladder position is derived; nothing carries mutable programme state (D2) |
| **§ 9** Deterministic derivation is code, override is data | PASS | Generation is a total function of authored data plus performed history. The quantisation rule is code; the increment it quantises to is data |
| **§ 10** Ordered within a source | PASS | Applied in the history projection (D3). Two records for one workout resolve to the later before anything is prescribed from them |
| **§ 11** Prescribed and performed separate and joinable | PASS | Separate tables, separate ports, one direction. `PrescribedSet` and `Set` are distinct types with deliberately inverted rest semantics; no port returns both. The projection runs the permitted direction only, and `WorkoutShape` vs `PrescribedWorkout` makes a projected shape unstorable as a prescription (D9) |
| **§ 12** Authored data durable, keeps history | PASS | Programme and parameters are versioned by authored-at date rather than overwritten; issued prescriptions are never deleted or rewritten |
| **§ 13** Interpretive parameters effect-dated | N/A | None consulted. The operator's zone is the one candidate and it is already 002's |
| **§ 14** Generation parameters need only the current value | PASS | Exactly the § 14 case, and the reason it holds is FR-025: the issued prescription records the concrete numbers, so a superseded percentage answers no question (SC-009) |
| **§ 15/16** Hexagonal, external systems behind ports | PASS | Four new driven ports, two driving. No `sqlx` or `toml` type in a port signature |
| **§ 17** Deterministic first | PASS | Progression, load derivation and scheduling are algorithms. No LLM anywhere in this feature — § 17 names these three explicitly |
| **§ 19** Frontend holds no domain logic | N/A | No frontend. `web` stays dormant |
| **§ 20/21** Rust only | PASS with note | SQL confined to the store; TOML confined to the authoring adapter. Complexity Tracking argues the case |
| **§ 23** No raw types at domain boundaries | PASS | `Anchor`, `Percentage`, `PlateIncrement`, `WeekIndex`, `Ladder`, `SessionRole`, `SlotId`, `TopSetReps`. No bare `u32` crosses a boundary |
| **§ 24** Illegal states unrepresentable | PASS | Doing real work here: `Prescribed` pins at least one axis by construction, so "prescribes nothing" is unwritable; `StrengthBlock`'s four named fields make a block missing a pattern unconstructible; a primary-style scheme on a non-primary slot has nowhere to live; `Performed::Failed` carries no count |
| **§ 25** Types document | PASS | `Anchor`, `SessionRole::Heavy`, `Performed::Failed` need no gloss |
| **§ 26** Errors typed, no panics | PASS | `PrescriptionError` for the run; an underivable slot is a value, not an error (FR-011). Panics are `forbid`, so fixture builders return `Result` |
| **§ 27** Types first | PASS | Task order is domain vocabulary → the outcome change → ports → store → generation → CLI |
| **§ 28** A random instance is valid | PASS | `proptest` over every new newtype, and over `Prescribed` — an arbitrary prescription pins an axis |
| **§ 29/30** Integration tests at ports are primary | PASS | Every [quickstart.md](./quickstart.md) scenario drives generation through its ports against the landed corpus |
| **§ 31** Red-green-refactor at the port boundary | PASS | SC-003 and SC-005 are written before the derivation and fail until the percentage tables and the reset arithmetic are right |
| **§ 32/33** Minimal scope, no proof-of-concept code | PASS | No canonical layer, no correspondence, no routine writing, no pattern vocabulary. Each is in the spec's Out of Scope with a reason |
| **§ 34** Deployment-agnostic | PASS | Database path, zone and the authoring document's path are all configuration |
| **§ 35** Credentials never in version control | N/A | This feature makes no request |
| **§ 36** A source unavailable degrades | PASS | Stronger: generation contacts no source. It works with Hevy down, against whatever was last extracted |
| **§ 37** Partial data recorded as partial | PASS | A slot with no history is reported as underivable rather than given a guessed load (FR-011). A failed attempt is recorded as a failure rather than as zero |
| **§ 38** Staleness observable | PASS | `fitness prescribe` reports the newest performance it read, so a prescription derived from stale history is visibly stale rather than quietly wrong |
| **§ 39/40** Review covers design; human sign-off | PASS | Branch and PR; not self-merged |

### Conflicts surfaced (Governance)

**One deviation, operator-decided, recorded rather than argued away.**

§ II.4 makes the canonical layer the one the analytical layer reads, and § II.5
builds analysis on top of it. This feature reads the normalised layer directly.
The operator settled this during specification on the grounds that one source is
in use and there is nothing to reconcile.

That reasoning is sound about *matching* and incomplete about *supersession*.
Normalised is not canonical even for one source, for two reasons — and the two
differ in how much evidence there is for them, which is worth being exact about:

- **Fragmentation is in the corpus.** One training session can be spread across
  several records; the gym-workout model records two days that landed four
  back-to-back records each. This is real and stays unresolved by this feature.
- **Supersession is not in the corpus.** All 165 landing records carry distinct
  source ids and nothing has been re-served. The case is possible under § 10 and
  is currently hypothetical, exactly as 001 and 002 both left it.

Matching is genuinely deferrable because there is no second source to match
against. Supersession is deferrable in practice today and not in principle: the
first re-served workout would put two versions of one performance in front of the
history projection, and prescribing from the withdrawn one is a silent wrong
answer rather than a visible failure. The cost of handling it now is one `WHERE`
clause; the cost of discovering it later is a wrong prescription nobody
questions.

**Resolution taken**: apply § 10's within-source rule inside the projection
(D3), and defer only cross-source correspondence. This is deterministic, needs no
overlay, and is the smallest thing that makes the read correct. It is not a
canonical layer and does not pretend to be one — fragmentation stays unresolved,
which is harmless for "the most recent performance of this exercise" and would
not be harmless for a session count. Recorded in
[decision 0006](../../docs/decisions/0006-prescription-reads-the-normalised-layer.md),
to be written with the implementation.

**One change of direction, needing its own decision record.** 002 specified a
zero-rep set as a refusal of kind `unmodelled`, and shipped it. This feature
makes it a failed attempt. That is not a correction of a defect but a reversal
made possible by the prescribed side existing — 002's own refusal comment says
the case "needs an *attempt*, which belongs with prescribed-versus-performed".
Recorded in
[decision 0007](../../docs/decisions/0007-a-zero-rep-set-is-a-failed-attempt.md).

**One conflict raised against an earlier draft and now dissolved.** § 12 lists
"assumed anchors" among authored data, which the first version of this plan had to
argue around, because it derived the anchor from the performed record. With the
anchor authored and fixed (D2) there is nothing to argue: § 12 is satisfied
literally. What is derived is the ladder and the position on it, which § 12 says
nothing about and § 7 asks for.

## Project Structure

### Documentation (this feature)

```text
specs/003-prescribed-workout-generation/
├── plan.md              # This file
├── spec.md
├── research.md          # Phase 0 — the eight decisions
├── data-model.md        # Phase 1 — the prescribed entities, and their SQL
├── quickstart.md        # Phase 1 — every scenario, as a runnable check
├── contracts/
│   ├── ports.md         # Five driven ports, two driving
│   ├── cli.md           # prescribe | programme, and status's new section
│   └── programme.md     # The template's composition, and the authoring document
└── tasks.md             # Phase 2 — /speckit-tasks, not created here
```

### Source Code (repository root)

```text
crates/
├── domain/                      ring 0
│   └── src/
│       ├── gym/
│       │   ├── set.rs           CHANGED — measure: M becomes outcome: Performed<M>
│       │   ├── outcome.rs       NEW — Performed<M>: Completed(M) | Failed
│       │   └── refusal.rs       CHANGED — RefusalReason::ZeroReps removed
│       └── prescription/        NEW — the prescribed side
│           ├── target.rs            Target<M>, Prescribed<M>, PrescribedSet<M>
│           ├── shape.rs             WorkoutShape, PrescribedItem, SlotId
│           ├── workout.rs           PrescribedWorkout — a shape, issued
│           ├── project.rs           GymWorkout -> Projection; satisfies()
│           ├── anchor.rs            Anchor, AnchorProvenance, StallCount, ResetStage
│           ├── quantise.rs          nearest-ties-down, over a load and an increment
│           ├── parameters.rs        GenerationParameters, Percentage, PlateIncrement
│           ├── schedule.rs          SessionRole, WeekIndex, the cycle calendar
│           ├── ladder.rs            Ladder — the block plan, and its positions
│           └── v1/                  the template and the programme rule
│               ├── template.rs      StrengthBlock, HypertrophyBlock, the five blocks
│               └── programme.rs     Programme, and generation over it
├── application/                 ring 1
│   └── src/
│       ├── ports.rs             + five driven, two driving
│       ├── error.rs             + PrescriptionError
│       └── prescribe.rs         NEW — the use case
├── infrastructure/              ring 2
│   └── src/
│       ├── hevy/
│       │   └── translate.rs     CHANGED — zero reps becomes Performed::Failed
│       ├── programme/
│       │   └── document.rs      NEW — the TOML authoring shape, and nothing else
│       └── store/
│           ├── history.rs       NEW — ExerciseHistory, with § 10 applied
│           ├── performed.rs     NEW — PerformedWorkoutReader, for projection
│           ├── parameters.rs    NEW — GenerationParameterStore
│           ├── programme.rs     NEW — ProgrammeStore
│           └── prescription.rs  NEW — PrescribedWorkoutStore
└── cli/                         ring 3
    └── src/
        ├── wiring.rs            + the prescription arm
        ├── output.rs            + rendering a prescribed workout
        └── main.rs              + prescribe, programme

migrations/
├── 0003_failed_attempt.sql      NEW
└── 0004_prescription.sql        NEW
.sqlx/                           regenerated — `cargo sqlx prepare --workspace`
docs/decisions/
├── 0006-prescription-reads-the-normalised-layer.md   NEW
└── 0007-a-zero-rep-set-is-a-failed-attempt.md        NEW
```

**Structure Decision**: no new crate. `crateRings` and the three-edit crate
process are untouched, as in 002.

Three placements are worth stating, because each is a rule this repository has
already been bitten by:

- **The template and the programme rule are `domain`.** They are the deterministic
  algorithm § 17 names, they hold no vendor identifier, and they are what
  `prescription/v1/` exists to version. Variants are append-only: `v1` is never
  edited once a programme runs against it.
- **The authoring document's shape is `infrastructure`.** A TOML representation is
  a serialisation concern, so `document.rs` holds the serde shapes and converts
  into `domain` types at the boundary. `domain` never sees a `toml` type, exactly
  as it never sees Hevy's JSON.
- **The history projection is a store adapter, not a use case.** It answers "the
  most recent performance of this exercise" in SQL over the normalised tables.
  Putting it in `application` would mean either loading 3,755 sets to filter in
  memory or leaking SQL out of the adapter ring.

**A fixture reminder that has cost this repository a green build before.**
`commonCargoSources` takes `.rs` and `Cargo.toml` and nothing else. The authored
programme used by the generation suites is a `.toml` fixture, so it must be named
in `flake.nix`'s fileset explicitly — the existing
`./crates/infrastructure/tests/fixtures` entry already covers the directory, so
placing it there is sufficient and placing it anywhere else is not.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
| --- | --- | --- |
| **A new runtime dependency, `toml`** (§ 20/21: no third language for application code) | FR-023 and FR-024 require the programme and its parameters to be stored, which means the operator needs a way to author them. § 21 exempts "interface languages confined to their adapter", and a document read once at the authoring boundary is that | Flags on a CLI command: the programme carries eleven slot fills, a warm-up ramp of four steps, per-role rep counts and two reset protocols — thirty-odd values, which is an unusable command line and an unreviewable one. Seeding through a migration is worse: authored data is not schema, and § 12 requires history a migration cannot express. JSON was the alternative to TOML and loses on being unreadable to edit by hand, which is the document's only purpose |
| **`Set<M>` changes shape**, and every consumer of `measure` with it | FR-028 to FR-030. A failed attempt has a load and no count, and the three ways to avoid changing the type all fail: a `SetKind::Failed` variant leaves the measure needing a value, a sentinel `RepCount` reintroduces the zero § 24 spent 002 removing, and an `Option<M>` cannot distinguish "failed" from "not recorded" | Leaving it as a refusal is the status quo and blocks Story 3 outright — a stall the programme cannot see. The change is mechanical and the compiler enumerates every site |

Two further costs, accepted rather than violations:

- **The anchor being derived costs a cycle calendar.** Deriving the anchor means
  knowing which cycles have passed, which means the programme carries a start
  date and a weekday-to-role mapping and the derivation walks it. Storing a
  current anchor would need none of that — and would need a rebuild story, a
  double-advance guard, and a reason to trust the number. D2 takes the calendar.
- **`Performed<M>` adds a match arm to every volume computation.** There are few
  today, which is the cheapest moment to pay it, and the arm is what makes FR-029
  a compile-time fact rather than a discipline.

## Post-Design Constitution Re-check

Re-run after Phase 1. No gate changed status; the § II.4 deviation stands as
recorded. Four observations from building the artifacts:

- **§ 11's "joinable" is doing more work than it looks.** The spec defers
  correspondence, so nothing joins prescribed to performed yet — but FR-025
  requires the issued prescription to carry the date it was issued for, and that
  date is the join key a later feature will use. Designing the table without it
  would have made correspondence a migration rather than a query. Added during
  design.
- **§ 24 nearly failed on the mobility block.** `Prescribed` pins at least one
  axis, and a couch stretch pins none — it has a duration and no load and no
  effort. The resolution is that a duration *is* a pinned measure, so
  `Fixed { load: Load::Relative(0), measure, effort: None }` is honest rather
  than an encoding trick. This closes the domain model's open question 4 as a
  side effect, and [data-model.md](./data-model.md) records why.
- **The history projection needed a "never performed" answer, not an empty one.**
  FR-011 distinguishes a slot whose exercise has no history from a slot that
  failed to derive. An `Option<Performance>` conflates them at the call site, so
  the port returns a type that names the case.
- **SC-003 is the mapping test of this feature**, and it is written before the
  percentage tables exist. It will fail loudly and specifically until the
  authored values are right, which is how the values get authored — the same
  mechanism 002 used for the exercise mapping's seven zeros.
- **The operator's round-trip observation changed the type factoring and found a
  missing port.** `PrescribedWorkout` originally bundled the items with the anchor
  and the date; splitting `WorkoutShape` out is strictly better and was not
  visible until a second producer of that structure existed. Writing the
  projection then exposed that `ExerciseHistory` as first drafted could not
  support the ladder position at all — it returned a latest value per exercise,
  where deciding whether the ladder advances, holds or suspends needs one
  exercise's whole series. Two reads on that port and a fifth port for whole
  workouts. This is the design phase earning its place: both gaps would have been
  found in implementation, more expensively.
- **The operator's reframing of the anchor collapsed a decision rather than
  complicating one.** Fixing the anchor removed the cycle-walking anchor
  derivation, removed a § 12 conflict this plan had been arguing around, removed
  the `anchor_per_week` parameter, and reduced D8's unknowns from a percentage
  table to a single ladder span. The design got smaller, which is the direction
  a revision should move in.

The gap design did not close: **the ladder's span remains unknown**, so
SC-003's expected values cannot be written until the operator supplies it. This
is recorded as an authored-value gap rather than a design one (D8), and it is the
one thing that stands between this plan and a workout.
