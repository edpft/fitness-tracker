# Contract: ports

Five driven ports and two driving ports, declared in `application` (§ 16, and the
standard hexagonal position: a port is defined by what the core needs).

Conventions inherited from 001 and 002 and not restated per port: every port is a
trait with `-> impl Future + Send` rather than `async fn`; no vendor type crosses a
signature, so no `sqlx`, `toml`, `serde_json` or `jiff` formatting type appears
below; errors are typed and adapters translate vendor failures at the boundary
(§ 26); a trait names a thing rather than an act.

---

## Driven ports

### `ExerciseHistory`

The projection of the performed record that prescription reads. The one place
§ 11's permitted direction is exercised: prescription may read the performed layer,
and never the reverse.

```rust
pub trait ExerciseHistory {
    /// The most recent working performance of each exercise asked about.
    ///
    /// Unbounded in time: an alternating slot's exercise was last performed two
    /// sessions ago, not one. The answer names `NeverPerformed` rather than
    /// returning nothing, because a slot with no history is prescribable and a
    /// slot that failed to derive is not.
    ///
    /// Where two landing records share a source record id, the later-served one
    /// is the one read (§ 10). Warm-up sets are excluded.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn last_performances(
        &self,
        exercises: &[RepsExercise],
    ) -> impl Future<Output = Result<BTreeMap<RepsExercise, LastPerformance>, StoreError>> + Send;

    /// Every working performance of one exercise, oldest first.
    ///
    /// The ladder derivation needs this and `last_performances` cannot supply
    /// it. Deciding whether the ladder advances, holds or suspends means asking of
    /// each gating session in turn whether its top set completed or failed, and
    /// whether a failed load had already been failed once — which is a series,
    /// not a latest value. Restricted to the primary in practice; the port does
    /// not enforce that, because nothing about the query depends on primacy.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn performances(
        &self,
        exercise: RepsExercise,
    ) -> impl Future<Output = Result<Vec<Performance>, StoreError>> + Send;

    /// The newest performance in the record, whatever exercise it was of.
    ///
    /// § 38: a prescription derived from stale history should be visibly stale.
    /// This is what the command prints alongside what it issued.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn newest_performance(&self) -> impl Future<Output = Result<Option<Date>, StoreError>> + Send;
}
```

**Batched deliberately.** `last_performances` takes every exercise the workout
needs in one call rather than one per slot. Eleven round trips to answer one
question is the shape that turns into an N+1 the first time a programme grows.

**Two reads, because two questions.** Double progression asks for a latest value
per exercise; the ladder asks for one exercise's whole series. Collapsing them
would mean either loading every exercise's full history to use one of them, or
deriving the ladder position from a latest value it cannot be derived from. The
first draft of this contract had only `last_performances`, and the ladder position
is what exposed the gap.

### `PerformedWorkoutReader`

```rust
pub trait PerformedWorkoutReader {
    /// Whole workouts in a date range, oldest first, § 10 applied.
    ///
    /// Needed to project a performance into a prescription shape, which operates
    /// on the workout entire — its items, its groupings, its ordering — where
    /// `ExerciseHistory` deliberately returns per-exercise summaries.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn between(
        &self,
        from: Date,
        to: Date,
    ) -> impl Future<Output = Result<Vec<GymWorkout>, StoreError>> + Send;
}
```

**Why this is separate from `ExerciseHistory`.** They answer different questions at
different grains, and merging them would give one port two reasons to change. This
one exists for the projection (research D9) and for SC-010's comparison across the
fifteen sessions. It returns the domain entity untouched, because projection is a
`domain` function over a `GymWorkout` and anything less than the whole workout
cannot supply the ordering and grouping the shape is made of.

§ 10 is applied here too, and for the same reason as in `ExerciseHistory`: two
records for one workout would otherwise both project.

**Why this is a store port and not a use case.** It is a query over the normalised
tables, expressible in SQL where the adapter lives. In `application` it would mean
either loading 3,755 sets to filter in memory or leaking SQL up a ring.

### `GenerationParameterStore`

```rust
pub trait GenerationParameterStore {
    /// The parameters in force — the greatest `authored_at`.
    ///
    /// § 14 requires only the current value. Superseded rows are retained and
    /// no derivation reads them; an issued prescription names the `authored_at`
    /// it used, which is what makes that safe.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    /// [`PrescriptionError::NoParameters`] if none have been authored.
    fn current(&self) -> impl Future<Output = Result<GenerationParameters, StoreError>> + Send;

    /// Author a set, superseding by date rather than overwriting (§ 12).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn author(
        &self,
        parameters: GenerationParameters,
    ) -> impl Future<Output = Result<(), StoreError>> + Send;
}
```

### `ProgrammeStore`

```rust
pub trait ProgrammeStore {
    /// The programme in force.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn current(&self) -> impl Future<Output = Result<Option<Programme>, StoreError>> + Send;

    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn author(&self, programme: Programme)
        -> impl Future<Output = Result<ProgrammeId, StoreError>> + Send;
}
```

**`Option` here and not on the parameters.** A programme that has never been
authored is the ordinary first-run state and the CLI has something helpful to say
about it. Absent parameters are the same case; both are reported, and the split is
only in where the error is constructed.

### `PrescribedWorkoutStore`

```rust
pub trait PrescribedWorkoutStore {
    /// Record what was issued, in full (FR-025). Written once, never rewritten.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn issue(
        &self,
        workout: PrescribedWorkout,
    ) -> impl Future<Output = Result<PrescribedWorkoutId, StoreError>> + Send;

    /// What was issued for a date, if anything.
    ///
    /// Read before issuing: asking twice for one date returns what was already
    /// issued rather than issuing a second prescription (FR-010).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something unreadable.
    fn issued_for(
        &self,
        date: Date,
    ) -> impl Future<Output = Result<Option<PrescribedWorkout>, StoreError>> + Send;
}
```

**FR-010 is satisfied twice over, and that is intentional.** A derived ladder position
means there is no counter to double-advance even if a second prescription were
issued (research D2). `issued_for` then makes the *output* idempotent as well, so
asking twice prints the same thing rather than a second, identical record. Belt and
braces, where the braces are structural and the belt is a query.

---

## Driving ports

### `WorkoutPrescriber`

```rust
pub trait WorkoutPrescriber {
    /// Issue the prescription for a date, or return what was already issued.
    ///
    /// The date is the only argument. The session role, the cycle index and the
    /// anchor are all derived — the role from the programme's calendar, the
    /// anchor from the entry anchor and the performed record. Passing any of them
    /// would be passing a derived value, which is the mistake
    /// `HevyWorkoutLandingStore::STREAM` exists to avoid on the extraction side.
    ///
    /// # Errors
    ///
    /// [`PrescriptionError`] — no programme, no parameters, the date falls on no
    /// programmed weekday, or the store is unavailable.
    fn prescribe(
        &self,
        date: Date,
    ) -> impl Future<Output = Result<Prescription, PrescriptionError>> + Send;
}

/// What the command got back, and enough to report it honestly.
pub struct Prescription {
    pub workout: PrescribedWorkout,
    pub freshly_issued: bool,
    /// § 38. The newest performance the derivation read.
    pub history_through: Option<Date>,
    /// Slots that could not be derived, and why (FR-011). Not an error: the rest
    /// of the workout is still worth issuing.
    pub underivable: Vec<UnderivableSlot>,
}
```

**`underivable` is a value, not an error.** FR-011 requires the system to say which
slot and why without substituting a guess, and a workout with ten good slots and
one gap is more useful than a refusal to answer. This is the same shape as 002's
`Translation`: a result carrying its own omissions.

### `ProgrammeAuthor`

```rust
pub trait ProgrammeAuthor {
    /// Take an authored programme and its parameters and store both.
    ///
    /// Takes `domain` types. The TOML document is converted in
    /// `infrastructure`, so nothing here knows a document format exists — which
    /// is what keeps § 21's exemption honest.
    ///
    /// # Errors
    ///
    /// [`PrescriptionError`] if the store is unavailable, or the programme is
    /// inconsistent in a way the types could not catch (see below).
    fn author(
        &self,
        programme: Programme,
        parameters: GenerationParameters,
    ) -> impl Future<Output = Result<ProgrammeId, PrescriptionError>> + Send;
}
```

**What the types cannot catch, and this must.** Most of the programme's validity is
structural — `SlotFills` is total, `PerRole` has both roles, `Range` needs
`low < high`. Three things are not:

1. The gating role must appear in the weekday mapping. A programme gating on a role
   it never runs would never advance.
2. The primary exercise must belong to the reps vocabulary. A duration primary has
   no top set.
3. The primary exercise must be the fill of the slot named by `primary`, or the
   programme names one exercise as primary and prescribes another.

---

## Errors

```rust
pub enum PrescriptionError {
    NoProgramme,
    NoParameters,
    /// The date falls on no weekday the programme runs. Names the days it does.
    NotAProgrammedDay { date: Date, programmed: Vec<Weekday> },
    InconsistentProgramme(/* one of the three above */),
    Store(StoreError),
}
```

An underivable slot is deliberately **not** here. It is a value on `Prescription`.

---

## What is deliberately not a port

**The projection of a performance into a prescription shape.** `project` and
`satisfies` are `domain` functions: total, synchronous, reading no store and making
no request. Nothing about them needs inverting, so a port would be ceremony — and
worse, a port implies an adapter, which implies a place where the rule could differ
by implementation. Research D9 and [data-model.md](../data-model.md) hold the
design; the only contract statement needed here is where they are *not*.

This also keeps § 11 clean at the port layer: no port hands out a
prescription-shaped value derived from performance. `PerformedWorkoutReader` yields
`GymWorkout` — a performed entity, unambiguously — and the caller applies a `domain`
function to it. The prescription shape comes into existence above the port layer and
never goes back down it, because no port accepts one except
`PrescribedWorkoutStore::issue`, which takes a `PrescribedWorkout` that a projection
cannot produce.

---

## What no port does

- **No port returns both prescribed and performed data.** § 11's separation is held
  in the type system: `ExerciseHistory` yields performed summaries,
  `PrescribedWorkoutStore` yields prescriptions, and nothing joins them, because
  correspondence is out of scope.
- **No port writes the performed layer.** Generation reads it and nothing more.
- **No port contacts a source.** Generation works with Hevy down (§ 36), against
  whatever was last extracted — which is exactly why `history_through` exists.
- **No port takes a session role or an anchor.** Both are derived. A driving port
  accepting either would let a caller prescribe a heavy session on a light day.
