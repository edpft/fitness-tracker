# Phase 1: Data model — Prescribed workout generation

Three groups: the one change to the performed side, the prescribed entities, and
the authored inputs. Reasoning that belongs to the model of record is not repeated
here — [`docs/prescribed-workout-domain-model.md`](../../docs/prescribed-workout-domain-model.md)
and [`docs/primary-lift-progression.md`](../../docs/primary-lift-progression.md)
own it. What is here is the projection into types and SQL.

---

## 1. The performed side: one change

### `Performed<M>`

```rust
/// What became of a set. Generic over the measure, like `Set<M>` itself.
pub enum Performed<M> {
    Completed(M),
    /// Attempted and not completed. Carries no count, by construction.
    Failed,
}
```

`Set<M>` changes one field:

```rust
pub struct Set<M> {
    pub load: Load,
    pub outcome: Performed<M>,   // was: measure: M
    pub intensity: Option<Rir>,
    pub kind: SetKind,
    pub rest_after: Option<Duration>,
}
```

**Invariants.** `Failed` holds nothing, so no arithmetic can extract a quantity
from a failure (FR-029). `RepCount` keeps its `NonZeroU32`, so zero remains
unrepresentable as a count — the failure is a different variant, not a zero.
`SetKind` is untouched: a failed attempt is a working set that failed, and
`SetKind` answers only whether volume metrics should count the set.

**Removed.** `RefusalReason::ZeroReps`. The corpus's refusals drop from three to
two (SC-006), and the two that remain are the malformed groupings, both `wrong
data`.

**Every consumer of `measure` changes.** The compiler enumerates them; there are
few today, which is why now is the cheapest moment. The rule at each site: a
`Failed` set contributes nothing to a total, a count or an estimate, and is never
silently skipped without that being the intended behaviour.

### SQL — `migrations/0003_failed_attempt.sql`

`performed_set` gains a discriminator. Sketch:

```sql
ALTER TABLE performed_set ADD COLUMN outcome TEXT NOT NULL DEFAULT 'completed'
    CHECK (outcome IN ('completed', 'failed'));
```

The `DEFAULT` exists so the column can be added to a populated table and is not
relied on afterward: the normalised layer is a derivation and the next run
replaces every row. The measure columns then carry the invariant SQLite can hold:

- `outcome = 'failed'` → `reps`, `duration_seconds` and `distance_mm` all NULL.
- `outcome = 'completed'` → exactly one of the three is non-NULL.

Which measure a failed set *would* have been is not lost; `performed_exercise.measure`
carries it, as it already does for every set of that entry.

Adding a `CHECK` to an existing table needs a table rebuild in SQLite, so the
migration is written as create-new, copy, drop, rename rather than a bare `ALTER`.
That is fine here for the reason above: the table holds a derivation, so even a
failed copy costs a re-derivation rather than a fact.

---

## 2. The prescribed entities

### Targets and prescriptions

```rust
pub enum Target<M> { Exactly(M), Range { low: M, high: M } }

pub enum Prescribed<M> {
    Fixed { load: Load, measure: Target<M>, effort: Option<Rir> },
    ToEffort { load: Load, effort: Rir, predicted: Option<Target<M>> },
    Autoregulated { measure: Target<M>, effort: Rir },
}

pub struct PrescribedSet<M> {
    pub prescription: Prescribed<M>,
    pub rest_after: Option<Target<Duration>>,
    pub warmup: bool,
}
```

**Invariants.**

- Every variant pins at least one axis, so "prescribes nothing" is unconstructible
  (FR-003). This is the § 24 work in this group.
- `Range { low, high }` requires `low < high` at construction. Equal bounds are
  `Exactly`, and there is no third state.
- `Autoregulated` is reachable but unreached: no programme against the current
  schema issues one. Variants are append-only because a v1 programme still
  generating still needs it.

**The primary's sets, concretely.** Warm-up steps and the top set and the back-offs
are all `Fixed`. The top set takes `Target::Exactly` and `effort: None` — load
pinned, count pinned, nothing open, which is what makes it pass or fail (FR-005).

**Rest inverts, against the performed side.** Performed `rest_after` is `Option`
because nobody recorded it. Prescribed `rest_after` is `Option` because no
instruction was given. Same shape, opposite meaning, which is why these are two
types rather than one shared one.

**Laterality is not a field.** The set boundary is the rest boundary, so ten each
leg then rest is one prescribed set of twenty. "Per leg" is recoverable from the
exercise being unilateral.

### The mobility block, and open question 4

`Prescribed` requires a load in two of three variants, and a couch stretch has
none. The resolution: a **duration is a pinned measure**, so a mobility slot is

```rust
Fixed { load: Load::Relative(0), measure: Target::Exactly(duration), effort: None }
```

and the pinned axis is volume, not intensity. `Load::Relative(0)` is what the
performed model already stores for unloaded work — [decision
0003](../../docs/decisions/0003-unrecorded-resistance-translates-as-relative-zero.md)
— so this is the same encoding on both sides of § 11 rather than a new one. That
closes the domain model's open question 4, which asked whether it was a fact or an
encoding: it is an encoding, used consistently, and the alternative — a fourth
`Prescribed` variant pinning only a measure — would be a variant whose only member
is stretching.

### The workout

```rust
/// Which slot an item fills. Survives into what is issued; block is derivable.
pub struct SlotId(/* stable key */);

pub enum PrescribedExercise {
    ForReps     { exercise: RepsExercise,     sets: NonEmpty<PrescribedSet<RepCount>> },
    ForDuration { exercise: DurationExercise, sets: NonEmpty<PrescribedSet<Duration>> },
    ForDistance { exercise: DistanceExercise, sets: NonEmpty<PrescribedSet<Distance>> },
}

pub struct PrescribedSuperset { pub members: AtLeastTwo<PrescribedExercise> }

pub enum PrescribedItem {
    Exercise { slot: SlotId, exercise: PrescribedExercise },
    Superset { slots: AtLeastTwo<SlotId>, superset: PrescribedSuperset },
}

/// What to do, and nothing about where it came from.
///
/// The common currency between a generated prescription and a performance
/// projected into prescription's vocabulary (research D9).
pub struct WorkoutShape { items: NonEmpty<PrescribedItem> }

/// A shape that was issued, and everything that makes that claim true.
pub struct PrescribedWorkout {
    shape: WorkoutShape,
    issued_for: Date,          // the date, in the operator's zone
    session_role: SessionRole,
    week: WeekKind,            // a ladder position, or the block's test
    anchor: Anchor,            // recorded concretely — § 14 depends on this
    parameters: GenerationParameters,
    programme: ProgrammeId,
    issued_at: Timestamp,
}
```

**Why the shape is a separate type, and not a convenience.** Only generation can
build a `PrescribedWorkout`, because only generation holds an anchor, a cycle and a
programme. A `WorkoutShape` produced by projecting a performance therefore has
nowhere to be stored as a prescription — FR-034 is a property of the types rather
than a rule to follow. The hazard this closes is a real one: a prescription
reverse-engineered from the performance it exists to be compared against would make
expectation-versus-reality unrecoverable, which is what § 11 protects. Research D9
has the full argument.

**Invariants.**

- The measure partition is reused from the performed side, so a prescribed set and
  its exercise cannot disagree and nothing validates the pairing.
- Items are slot-tagged (FR-009), or "same slot, different cycle" stops being
  answerable. A superset tags each member.
- Blocks do not survive into what is issued. They are construction-time
  scaffolding; block is derivable from slot.
- `issued_for` is a date, not an instant. It is also the join key a later
  correspondence feature will use — designing the table without it would make
  correspondence a migration rather than a query.
- The anchor and the parameters are recorded **by value**. This is what makes § 14
  correct: only the current parameter is required precisely because what it
  generated is captured here (SC-009).

**No `Session`, no attempt entity.** Fragmentation is an artefact of observation
and is not prescribed. A failed attempt is a performed fact, and D1 put it there.

### Projection: a performance in prescription's vocabulary

```rust
/// Total, and `domain`. Reads no store, makes no request, consults no overlay —
/// which is why it is a function and not a port.
pub fn project(workout: &GymWorkout) -> Projection;

pub struct Projection {
    pub shape: WorkoutShape,
    /// What the performed record could not supply. Never filled with a guess.
    pub gaps: Vec<ProjectionGap>,
}

pub enum ProjectionGap {
    /// A failed attempt carries the load and not the repetitions intended.
    IntendedMeasureUnknown { at: ItemPosition, load: Load },
    /// The structure did not match the template, so no slot could be assigned.
    SlotUnassignable { at: ItemPosition },
}
```

**Comparison is direction-aware, and that is a domain property.** A performed six
repetitions projects to `Target::Exactly(6)`, but the prescription may have said
4–6. So comparing a projection against a generated prescription treats a projected
`Exactly(n)` as agreeing with a prescribed `Range { low, high }` when
`low <= n <= high`. Equality on `WorkoutShape` is the wrong relation and is not the
one the comparison uses; the relation is *satisfies*, and it is asymmetric.

```rust
/// Does this performance's shape satisfy that prescription's?
pub fn satisfies(performed: &WorkoutShape, prescribed: &WorkoutShape)
    -> Vec<Divergence>;
```

An empty `Vec` is agreement. A non-empty one lists what differed — a swapped
exercise, a reordered item, a set count short, a load off. It reports rather than
judges: neither side is authoritative, because a session legitimately diverges from
its prescription and the projection cannot know which divergences were intended.

**Three things the projection deliberately drops.** Each would be promoting an
observation into an instruction:

- **Observed RIR does not become prescribed effort**, except where the
  prescription's own shape is `ToEffort`, in which case it lands in `predicted`.
- **Recorded rest does not become a rest instruction.** Performed rest is what
  happened; the corpus records almost none of it anyway.
- **Slot identity is assigned by position against the template, not invented.**
  Where the structure diverges, `SlotUnassignable` is recorded — which is precisely
  the signal that a session was restructured at the gym.

### The primary's shape, and the anchor

```rust
pub struct Anchor { load: Kg, provenance: AnchorProvenance, from: Date }

pub enum AnchorProvenance { Tested, Estimated, Asserted }

pub enum SessionRole { Light, Heavy }

pub struct WeekIndex(NonZeroU32);
```

**The anchor is authored and constant for the block** (research D2). It is stored
once on the programme and copied onto each issued prescription. Nothing derives it,
and nothing within a block changes it.

### The ladder

```rust
/// The block's plan: a percentage of the anchor per climbing week.
pub struct Ladder {
    start: Percentage,
    end: Percentage,
    climbing_weeks: NonZeroU32,   // duration − 1; the last week is the test
}

impl Ladder {
    /// The heavy top set for a climbing week. `None` for the test week.
    pub fn heavy_top_set(&self, anchor: Kg, week: WeekIndex) -> Option<Kg>;
}

pub enum WeekKind { Climbing(WeekIndex), Test }
```

**The step is authored, never derived**: each climbing week is one
`ladder_climb_per_week` above the last, opening at the load the entry test failed
— or one climb above what it completed, if it failed nothing (D14).
Revised 2026-08-19 — this said the reverse, and derived the step by dividing a
span by `climbing_weeks − 1`. See
`docs/decisions/0008-the-linear-ladder-climbs-at-a-rate.md`.

So the same plan over 8 or 12 weeks is one plan run further, and duration says how
long the climb runs rather than shaping it. That is the loss the decision record
accounts for, and it is why `block` rather than `linear` is the template whose
duration means something.

**A single climbing week is representable and degenerate** — the ladder is one
position, its opening, with no week in which to climb. There is no longer a
divisor that could be zero.

### Ladder position, and what failure does to it

The anchor does not move, so the state machine is about *position on the ladder*:

| From | Event | To |
| --- | --- | --- |
| On the ladder at week *w* | top set completed | week *w+1* |
| On the ladder at week *w* | top set failed | week *w* re-issued, no advance |
| Week *w* re-issued | same load failed again | ladder **suspended**; reset 1 from the failed load, −10% at +5kg/week |
| In a reset re-climb | re-climb reaches the failed load | ladder **resumed** at week *w* |
| In reset 1 | same load failed twice again | reset 2, −5% at +2.5kg/week |
| any | no session occurred | unchanged, no stall accrued |
| any | test week reached | the block ends; the tested value anchors the next block |

Three rows carry the design:

- **The anchor appears nowhere in this table.** A reset drops from the *failed
  load*, not from the anchor. A stall is evidence the plan was too ambitious, not
  evidence about where the block started (FR-021).
- **Resume, not restart.** When a re-climb returns to the load that was failed, the
  ladder picks up at the week it was suspended at (FR-020). The reset is a detour,
  not a rewind.
- **Absence is not a miss.** No stall accrues and the week re-issues, which is what
  D1 exists to make visible.

**What marks a session as a test** is programme information, not an inference from
the sets. A ramp to a single that then fails is indistinguishable from a heavy
session that missed, unless the programme says a test was scheduled. Recorded here
because it is easy to assume the sets can tell you.

### Quantisation

```rust
/// Nearest multiple of the increment; an exact tie resolves down.
pub fn quantise(load: Kg, increment: PlateIncrement) -> Kg;
```

A `domain` function over a load and an increment, not a method on a back-off.
Applied to back-offs, warm-up ramp steps and reset drops alike (research D5). No
float on the path: `Kg` is fixed-point over `i64` grams, inherited from 002, and
percentage application is integer arithmetic with the rounding done explicitly
rather than by a cast.

Worked cases, which are also the property test's anchors: 68 → 67.5 (nearest),
78.75 → 77.5 (exact tie, down), 74.375 → 75 (nearest), 72.25 → 72.5 (nearest).

### SQL — `migrations/0004_prescription.sql`, prescribed half

Four tables mirroring the performed shape, because what is issued has the same
structure as what is recorded:

Only `PrescribedWorkout` has a SQL projection. `WorkoutShape` has none of its own:
it is stored as the item and set rows hanging off a prescription, and a projected
shape is never stored at all — there is no table it could go in, which is FR-034
held at the schema as well as in the types.

```text
prescribed_workout   id, programme, issued_for (date), zone, session_role,
                     cycle_index, anchor_grams, anchor_provenance, anchor_from,
                     parameters_authored_at, issued_at
prescribed_item      workout, position, is_superset
prescribed_slot      workout, item_position, member_position, slot
prescribed_exercise  workout, item_position, position, exercise, measure
prescribed_set       workout, item_position, exercise_position, position,
                     variant, load_kind, load_grams,
                     target_kind, target_low, target_high,
                     effort, rest_low_seconds, rest_high_seconds, warmup
```

**These carry no append-only trigger and no wholesale replacement.** Both would be
wrong. They are § III.12 authored data: nothing regenerates them if lost, so they
are durable and keep history — but they are not raw either, so the triggers that
guard raw do not belong. An issued prescription is written once and never rewritten.

`CHECK` constraints hold what the types hold: the `variant` and the columns that
must be present together, so `Prescribed`'s "pins at least one axis" is
unrepresentable in the file as well as in Rust — including to a writer that is not
this program.

---

## 3. The authored inputs

### Generation parameters

```rust
pub struct Percentage(/* basis points, integer */);
pub struct PlateIncrement(Kg);
pub struct TopSetReps(NonZeroU32);

pub struct WarmupStep { pub of_top_set: Percentage, pub reps: RepCount }

pub struct ResetProtocol { pub drop: Percentage, pub reclimb_per_week: Kg }

pub struct GenerationParameters {
    pub warmup: NonEmpty<WarmupStep>,
    pub back_off_of_top_set: Percentage,
    pub top_set_reps: PerRole<TopSetReps>,
    /// What the climb adds each climbing week. There is no endpoint, and no
    /// opening either: the entry test on the `Anchor` says where it starts.
    pub ladder_climb_per_week: Kg,
    /// The light session's top set, as a percentage of that week's heavy one.
    pub light_of_heavy: Percentage,
    pub plate_increment: PlateIncrement,
    pub first_reset: ResetProtocol,
    pub second_reset: ResetProtocol,
    pub authored_at: Timestamp,
}
```

**`anchor_per_week` is gone.** It was the first version's climbing increment, and
it moved the *anchor* — which re-bases the warm-ups and the back-offs with it, so
no two weeks are comparable. `ladder_climb_per_week` is the rate it was reaching
for, applied to the ladder's position instead, with the anchor left where the test
put it.

**`light_of_heavy` replaces a per-role percentage of the anchor.** The light session
is derived from that week's heavy top set, so the two move together by construction
and one ladder serves both roles.

**The value is 85%, and the value it replaced is a cautionary tale.** It was 88.5%,
solved from the record's three validated weeks — 82.5/85/87.5 heavy against
72.5/75/77.5 light. Every one of those pairs is a flat −10kg; the percentage was a
ratio fitted to an offset, reproducing all three only because quantisation rounds it
back onto the plate grid, and drifting across them (87.9%, 88.2%, 88.6%) where the
offset does not drift at all. The operator stated 85% on 2026-08-18. A percentage is
still the right shape — an offset is a far larger relative drop at a 60kg anchor than
at a 90kg one — but the number in it has to be chosen rather than solved for.

**Percentage is integer.** Basis points, not a float — the same reasoning as `Kg`.
A percentage that round-trips differently across builds would make a stored
prescription unreproducible.

**`PerRole<T>` is a struct with a field per role**, not a map. Two roles exist and
both must be present; a map makes a missing role a runtime error where a struct
makes it a compile error (§ 24).

**Only the current value is required** (§ 14). Superseded rows are kept because
they cost nothing and because an issued prescription names the `authored_at` it
used, but no derivation consults a superseded one.

### The programme

```rust
pub struct Programme {
    pub id: ProgrammeId,
    pub primary: PrimaryPattern,        // which strength slot is primary
    pub primary_exercise: Exercise,
    pub fills: SlotFills,               // total over the template's slots
    /// The starting 1RM. Constant for the block; replaced only by its exit test.
    pub anchor: Anchor,
    pub gating_role: SessionRole,
    pub start: Date,
    pub weekdays: NonEmpty<(Weekday, SessionRole)>,
    /// Weeks, not cycles. The last one is the test.
    pub duration_weeks: NonZeroU32,
    pub authored_at: Timestamp,
}
```

**`duration_weeks`, and the anchor, are the two inputs the whole primary loading
series is a function of.** That is what makes "a number of weeks and a starting 1RM"
a complete statement of the generator's job — everything else on this struct is
about which exercises fill which slots, not about the plan.

**`anchor`, not `entry_anchor`.** The rename matters: "entry" implied something that
moves after entry. Nothing does, within a block.

**`SlotFills` is total over the template's slots**, and the template is a struct
with a named field per slot — so a programme missing a fill does not compile. That
is the same mechanism as `StrengthBlock`'s four patterns:

```rust
pub mod linear {
    pub enum PrimaryPattern { KneeDominant, HipDominant, UpperPush, UpperPull }

    pub struct StrengthBlock {
        pub knee_dominant: Exercise,
        pub hip_dominant: Exercise,
        pub upper_push: Exercise,
        pub upper_pull: Exercise,
        pub primary: PrimaryPattern,
    }

    pub struct HypertrophyBlock {
        pub arms: Superset,
        pub forearms: Superset,
        pub core: Exercise,      // single, never supersetted
    }
}
```

**A fill may alternate by role.** The hip-dominant slot runs Nordic curls on one
session and the back-extension machine on the other, so a fill is either one
exercise or one per role. This is what makes the history projection's unbounded
lookback necessary rather than merely tidy (research D4).

**No `Pattern` field beside the slot names.** The field name *is* the pattern; a
second source of truth could disagree. Pattern as exercise vocabulary is out of
scope for this feature entirely.

### SQL — `migrations/0004_prescription.sql`, authored half

```text
generation_parameters      authored_at (PK), back_off_bp, plate_increment_grams,
                           ladder_climb_grams, light_of_heavy_bp,
                           reset1_drop_bp, reset1_reclimb_grams,
                           reset2_drop_bp, reset2_reclimb_grams
generation_warmup_step     parameters_authored_at, position, of_top_set_bp, reps
generation_role_reps       parameters_authored_at, role, top_set_reps
programme                  id (PK), authored_at, primary_pattern,
                           primary_exercise, anchor_grams,
                           anchor_provenance, anchor_from,
                           gating_role, start_date, duration_weeks
programme_slot_fill        programme, slot, role (NULL = both), exercise
programme_weekday          programme, weekday, role
```

Superseded rows are retained. "Current" is the greatest `authored_at`, which is a
`WHERE` clause rather than a mutable flag — the same reasoning that keeps the
normalised layer free of a `is_current` column.

---

## 4. The read side

```rust
/// What the projection returns per exercise. Not `Option`.
pub enum LastPerformance {
    Performed {
        on: Date,
        landed_as: LandingRecordId,
        sets: NonEmpty<PerformedSetSummary>,
    },
    NeverPerformed,
}

pub struct PerformedSetSummary {
    pub load: Load,
    pub outcome: Performed<RepCount>,
    pub kind: SetKind,
}
```

`NeverPerformed` is a named case rather than an absent one (research D4), because
FR-011 needs a slot with no history to be prescribable and distinguishable from a
slot that failed to derive.

**The projection applies § 10.** Where two landing records share a source record
id, the later-served one wins — `serve_ordinal` already exists on the landing table
for exactly this. Warm-up sets are excluded; double progression reads working sets.

**Reps only.** Double progression applies to the reps vocabulary. Duration and
distance slots are static in this programme (mobility, plyometric), so the
projection is typed to `RepCount` rather than generic — a narrower port that says
what it does, and widened when a programme progresses a timed slot.
