# Data model: Hevy workout normalisation

Phase 1. The entity is `docs/gym-workout-domain-model.md`'s, with the two
amendments decisions 0002 and 0003 made. This document says how it is typed in
Rust and how it projects into SQLite, and nothing about why — the model of
record holds the reasoning and is not restated here.

Everything in "The entity" lives in `crates/domain/src/gym/` and depends on no
workspace crate.

*Kept in step with the code as it was written; where the two ever disagree the
code is the one that runs.*

---

## Value types

| Type | Carries | Rejects at construction |
| --- | --- | --- |
| `Kg` | `i64` grams | Anything not a decimal number; more than three decimal places; a value below zero |
| `SignedKg` | `i64` grams, signed | The same, without the sign rule |
| `Metres` | `i64` millimetres | Negative; non-decimal |
| `Duration` | `i64` seconds | Negative |
| `RepCount` | `u32` | Zero — a set of no reps is not a set (SC-002's one unmodelled case) |
| `Rir` | one of eight positions | Anything else. No `From<u8>`; the RPE mapping is the adapter's |
| `SetKind` | `Working` \| `Warmup` | — |

`Kg` and `SignedKg` are distinct types rather than one signed type with a
predicate, because the thing that must be impossible is an absolute zero, and a
predicate is checkable rather than impossible.

Every one of them implements `TryFrom<String>`, and gets `as_str`/`AsRef`/
`Display`/`TryFrom<&str>`/`FromStr` from `domain::landing::newtype`'s macros —
which move up to `domain::newtype`, since two modules now use them.

### Load

```rust
enum Load {
    Absolute(Kg),        // Kg cannot be zero when reached this way — see below
    Relative(SignedKg),  // 0 is plain bodyweight; negative is assistance
}
```

`Load::absolute` is a fallible constructor returning
`Err(RefusalReason::ZeroOnAbsoluteLoad)` for zero. `Kg` itself permits zero,
because a zero *distance* and a zero *bodyweight delta* are both meaningful and
the same carrier serves them; what is impossible is an absolute load of zero,
and that is enforced where it means something.

### Measures

```rust
struct Distance      { metres: Metres }
struct TimedDistance { metres: Metres, duration: Duration }
```

Four measures: `RepCount`, `Duration`, `Distance`, `TimedDistance`. The split of
the last two is [decision 0002](../../docs/decisions/0002-distance-and-distance-over-time-are-different-measures.md).

## The entity

```rust
struct Set<M> {
    load: Load,
    measure: M,
    intensity: Option<Rir>,
    kind: SetKind,
    rest_after: Option<Duration>,   // always None from this source
}

enum RepsExercise          { /* ~120 */ }
enum DurationExercise      { AirBike, CouchStretch, NinetyNinety, DeadHang,
                             Stretching, JumpRope, HandstandHold, SledPush }
enum DistanceExercise      { FarmersWalk, WalkingLunge }
enum TimedDistanceExercise { Running }

enum PerformedExercise {
    ForReps          { exercise: RepsExercise,          sets: NonEmpty<Set<RepCount>> },
    ForDuration      { exercise: DurationExercise,      sets: NonEmpty<Set<Duration>> },
    ForDistance      { exercise: DistanceExercise,      sets: NonEmpty<Set<Distance>> },
    ForTimedDistance { exercise: TimedDistanceExercise, sets: NonEmpty<Set<TimedDistance>> },
}

struct Superset { members: AtLeastTwo<PerformedExercise> }

enum WorkoutItem { Exercise(PerformedExercise), Superset(Superset) }

struct GymWorkout {
    items:            NonEmpty<WorkoutItem>,
    started_at:       WorkoutStart,
    provenance:       Provenance,
    source_record_id: SourceRecordId,
    landed_as:        LandingRecordId,
}
```

Four invariants are carried by the types and are therefore not checked anywhere:

- **A performed exercise has at least one set** — `NonEmpty`.
- **A superset has at least two members** — `AtLeastTwo`. The corpus's
  single-member grouping is unrepresentable, not merely rejected.
- **A workout has at least one item** — `NonEmpty`. A record whose every item
  refuses yields no workout, which is the spec's edge case rather than an empty
  one.
- **A set's measure is fixed by its exercise's vocabulary** — the type
  parameter. `Set<RepCount>` cannot reach a `DurationExercise`, so SC-011 holds
  by construction and an arbitrary instance is valid (§ 28).

Contiguity is *not* a type invariant — a `Superset` holds its members and knows
nothing of the sequence it came from. It is checked once, in the translator,
against the source's ordering, and a failure is a refusal. Nothing downstream
can break it, because nothing downstream can reorder a workout's items.

### Time

```rust
struct WorkoutStart { instant: Timestamp, zone: TimeZone }
```

There is no constructor taking an instant alone, so a naive timestamp cannot
reach the entity (§ II.3, FR-019). `wall_clock()` resolves through the zone, so
calendar bucketing is right across both switchovers. `jiff` supplies both, and
`Provenance` is `domain::landing`'s existing type — reused, not re-declared,
because provenance does not change shape when a payload is interpreted.

## Refusal

```rust
enum RefusalLocus {
    Record,
    Entry    { entry: u32 },
    Set      { entry: u32, set: u32 },
    Grouping { group: u32 },
}

enum RefusalReason {
    ZeroOnAbsoluteLoad,          // wrong data
    BandResistance,              // declared limitation
    ZeroReps,                    // unmodelled case
    NonContiguousGrouping,       // wrong data
    SingleMemberGrouping,        // wrong data
    NoSetsInEntry,               // wrong data
    UnknownSetKind { kind: String },        // the source's word, kept verbatim
    UnrecognisedIntensity { value: String },
    UnreadableValue { field: &'static str, detail: String },
    NothingTranslatable,         // every item refused; the record yields no workout
    UnreadablePayload { detail: String },
}

struct Refusal {
    landed_as: LandingRecordId,
    source_record_id: SourceRecordId,
    locus: RefusalLocus,
    /// Which of ours the refused thing belonged to, where that was known by the
    /// time it was refused. A position alone sends the operator back to the
    /// payload to find out what exercise 4 was, which is what FR-022 says a
    /// refusal must save them.
    exercise: Option<Exercise>,
    reason: RefusalReason,
}
```

`RefusalReason::kind()` returns `WrongData | DeclaredLimitation | Unmodelled`,
which is the distinction the model of record says telling apart is the point,
and which SC-007 requires an operator to see without re-reading the payload.

## The translation result

```rust
enum Translation {
    Workout { workout: Box<GymWorkout>, refusals: Vec<Refusal> },
    Retraction { of: SourceRecordId },
    Refused(NonEmpty<Refusal>),
}
```

The three outcomes of FR-005, as a sum. A record that produced no workout and no
reason does not compile, and a retraction cannot carry refusals — the two
mistakes most worth making impossible, since either would let a record go
silently missing.

## SQLite projection

`migrations/0002_normalisation.sql`. The tables mirror the sum types with
`CHECK` constraints, the same way `extraction_run` mirrors `RunOutcome` — so the
invariants hold against a writer that is not this program.

```text
normalisation_run(id, stream, started_at, finished_at, outcome,
                  records_read, workouts_written, workouts_withdrawn,
                  retractions_applied, records_refused, refusals_recorded,
                  failure_reason)

gym_workout(landing_record_id PK  -> hevy_workout_landing(id),
            source_record_id, started_at_utc, zone,
            endpoint, event_kind, event_time, run_id)

workout_item(workout, position, is_superset,
             PRIMARY KEY (workout, position))

performed_exercise(workout, item_position, position, exercise, measure,
                   PRIMARY KEY (workout, item_position, position))

performed_set(workout, item_position, exercise_position, position,
              load_kind, load_grams, reps, duration_seconds, distance_mm,
              rir, set_kind, rest_after_seconds,
              PRIMARY KEY (workout, item_position, exercise_position, position))

normalisation_refusal(id, run_id, landing_record_id, source_record_id,
                      locus_kind, entry_index, set_index, group_id,
                      exercise, reason, kind, detail)
```

Three notes on the projection:

- **No append-only triggers.** They guard an input; a derivation must be
  replaceable, and D6 replaces it wholesale in one transaction.
- **`performed_set`'s measure columns are nullable and `CHECK`-constrained** so
  that exactly the column its `measure` names is populated. This is the sum type
  written out flat, which is what a relational store can hold — the tempting
  mistake would be reading it back as "whichever column is filled", and the
  translator never does: the exercise says which measure applies, and the
  column follows from that.
- **`gym_workout.landing_record_id` is the key** (D8), so two records for one
  workout store two rows and neither shadows the other. Supersession stays
  unresolved here, as § 10 requires.

## What is deliberately absent

No `Session`, no correspondence, no metric, no overlay table, and no column
anywhere for a workout's title, description, notes or `routine_id`. Raw retains
all of it (§ II.1) and a later feature can read it; carrying it here would be
this layer holding what nothing in it is a function of.
