# Gym Workout Domain Model

*The gym-workout entity, and the reasoning behind it. Companion to the
constitution, which it does not override.*

*Modelled from what is true about training, then run against every landed Hevy
record: 164 records — 163 `updated`, 1 `deleted` — covering November 2024 to
August 2026, 1,135 exercise entries, 3,779 sets, 134 distinct exercise
templates. Counts quoted throughout are from that corpus, and what the run found
is in "Checked against the data" below. No source's shape decides anything here;
where Hevy appears, it appears as evidence about one adapter.*

---

## Entity inventory

Six entities. One is modelled here; the rest are named so the boundaries are
visible.

| Entity | Source | Shape |
|---|---|---|
| **Gym workout** | Hevy | Session → workouts → exercises → sets. |
| **Cycling workout** | Peloton | Summary + per-second sample stream, also session-wrapped. |
| **Body measurement** | Withings | One event, many metrics, one instant. Flat. |
| **Sleep** | Garmin | An interval crossing a date boundary, with stage durations. |
| **Daily physiological reading** | Garmin | HRV, RHR. One value per day, no parent session. |
| **Nutrition** | MacroFactor | A day's intake, plus a modelled expenditure estimate. |

- Body measurement and daily reading have no parent — they are § II.2's
  "degenerate entity". Whether a day is a container for them is open question 1.
- Nutrition's expenditure estimate is modelled, not measured, and may not belong
  in the same entity as intake.
- Sleep breaks the wall-clock timestamp assumption cleanly: a night is an
  interval, not an instant, and it crosses a date boundary. It attaches to the
  following day, because what it bears on is the next day's training.

**The session is not gym-specific.** Peloton offers warm-up and cool-down rides
as separate rides, so an FTP test is two of them — a warm-up and the test. That
is the same composition as a gym session made of several workouts, and it comes
from the source's own design rather than from a logging habit.

**It is a gym workout, not a strength workout.** 208 sets in the corpus are
running, skipping, sled work, stretching and isometric holds. What the entity is
does not depend on which device recorded it: a gym session recorded on a Garmin
watch as a strength activity and the same session recorded in Hevy are two
observations of one event, reconciled at § 4. Bike work inside a gym session
stays part of that session whichever of them recorded it.

---

## The types

```rust
enum Load {
    Absolute(Kg),         // the implement has mass; zero is impossible
    Relative(SignedKg),   // delta against bodyweight; 0 = plain bodyweight
}

struct Distance { metres: Metres, duration: Option<Duration> }

/// Reps in reserve. An ordinal scale: positions order and compare, they do not
/// average or subtract.
enum Rir {
    Zero, ZeroOrOne, One, OneOrTwo, Two, TwoOrThree, Three, FourOrMore,
}

enum SetKind { Working, Warmup }

struct Set<M> {
    load: Load,
    measure: M,
    intensity: Option<Rir>,
    kind: SetKind,
    rest_after: Option<Duration>,
}

/// The exercise vocabulary, partitioned by what its members are measured in.
enum RepsExercise     { FrontSquat, Deadlift, PullUp, ChestDip, BoxJump, /* … */ }
enum DurationExercise { DeadHang, HandstandHold, Stretching, /* … */ }
enum DistanceExercise { Running, FarmersWalk, WalkingLunge, /* … */ }

enum PerformedExercise {
    ForReps     { exercise: RepsExercise,     sets: NonEmpty<Set<RepCount>> },
    ForDuration { exercise: DurationExercise, sets: NonEmpty<Set<Duration>> },
    ForDistance { exercise: DistanceExercise, sets: NonEmpty<Set<Distance>> },
}

struct Superset { members: NonEmpty<PerformedExercise> }   // two or more, back to back

enum WorkoutItem { Exercise(PerformedExercise), Superset(Superset) }

struct Workout { items: NonEmpty<WorkoutItem> }            // ordered
struct Session { workouts: NonEmpty<Workout> }             // canonical layer
```

---

## Why each decision went the way it did

**Load is a property of every set, not a kind of set.** Every set has a load —
a barbell's, a stack's, or your own bodyweight — so it cannot also be the thing
that distinguishes one sort of set from another. A front squat and a box jump
are both sets of reps; what differs is that one is `Absolute(77.5)` and the
other `Relative(0)`.

This is worth stating because the opposite is tempting. Hevy has eight exercise
types, four of which differ only in whether and how a weight column is
populated, and every template in the corpus uses exactly one of them. That is a
real regularity — and a fact about Hevy's storage, not about training. A
categorisation that only tracks which columns a source fills is that source's
schema wearing domain vocabulary.

**Absolute vs Relative is decided by whether zero is performable.** A load is
`Relative` where an unloaded version of the movement exists — a pull-up, a
bodyweight Bulgarian split squat, a plain crunch — so zero is a real
observation and the number is a delta against a bodyweight the set does not
record. It is `Absolute` where the implement itself has mass, so zero is
impossible and the number is the whole load.

The rule earns its place by self-checking: on an absolute-load exercise a zero
is a data error *by construction*, with no judgement required. It is also what
keeps a 100kg squat and a bodyweight dip +10kg from being read as the same kind
of number.

**Assistance is negative, not a separate case.** Assisted −20 and weighted +10
sit on one axis, and the crossover through zero is a genuine progression that
must not change type. This is the motivating case for the whole load model:
collapsing "unassisted pull-up" and "pull-up with 0kg assistance" into one
series. The corpus is full of it — `Pull Up` (97 sets), `Pull Up (Assisted)`
(159), `Chest Dip` (84), `Chest Dip (Assisted)` (277) — and the collapse is why
`PullUp` is one exercise carrying a load, with a plain pull-up translating to
`Relative(0)` rather than to an absence.

**There is no "unrecorded" load.** It was tried and removed. It merged three
unrelated things: data that is simply wrong, a load that does not apply, and a
load that applies and was never captured. Worse, it could not catch the case it
was added for — deterministic translation cannot tell a zero that means
"unrecorded" from one that means "no added load", which is why that judgement
belongs to the mapping. A variant for absence would have absorbed the empty-bar
warm-ups below and hidden the pattern that makes them diagnosable.

**Bands are not modelled.** Band tension varies through the range of motion, so
no scalar is honest. Nothing available records one anyway: assistance arrives as
a bare number with no mechanism attached, and the account's assisted loads run
`0, 7, 14, 21, 28, 35, 42` — stacked bands rather than a machine stack, which
deterministic translation (§ 9) cannot distinguish. Band-named sets carry
operator estimates, not measurements. Band and machine assistance are therefore
not comparable, and that is a limitation to declare rather than one the value
can express.

**Three measures, because there are three things you can count.** Repetitions,
elapsed time, and ground covered. Everything else a source offers is one of
those recorded more or less fully.

Distance carries an optional duration rather than splitting into distance-and-
distance-over-time: a 20 m farmer's walk and a 200 m run in 60 s are the same
measure, one of them partial (§ 37). Whether that is right is open question 2 —
the case against is that a carry is time under load and a run is pace, and those
may not want to sit on one axis.

The partition is by measure alone, which is what makes a stored measurement type
unnecessary: an exercise's measure is fixed by which vocabulary it belongs to,
so a set and its exercise cannot disagree and nothing needs validating (§ 24),
and an arbitrary instance is valid by construction (§ 28).

Where a source's category and ours differ, ours wins. Hevy calls `Sled Push`
distance-and-duration; what it actually holds is thirty seconds and a zero
distance, so it is a duration exercise here.

**Intensity is RIR on an ordinal scale.** Hevy records RPE and glosses it as
reps in reserve in its own interface (10 = 0, 9.5 = maybe 1, 9 = 1, 8.5 = maybe
2 …), and it is used that way, which makes RIR the recorded fact.

The positions are not numbers. Modelling them as numbers produces two errors at
once: an `AtLeast(n)` that admits "at least one in reserve", which nobody
records, and a `Between(a, b)` that admits `Between(8, 7)`. Eight named
positions admit neither, order without arithmetic, and make "mean RIR across the
block" fail to compile — which is correct, because averaging an ordinal scale is
not meaningful.

`FourOrMore` is the last position rather than an open bound applied generally:
below four in reserve, precision is not claimed. Coverage is 2,415 of 3,779
sets, stable across all three years, and 984 of those are the uncertain
positions.

**Set kind is `Working` or `Warmup`, and nothing else.** Those are the two
states the domain distinguishes, because volume metrics need warm-ups excluded
and nothing else about a set's kind changes what it means. 361 sets are
warm-ups, and no positional rule reconstructs them — the corpus opens workouts
with heavy bridging singles tagged `warmup`.

A source's own kinds are not domain kinds. Hevy's `failure` and `dropset` are
both working sets to the only question asked of the field; a set taken to
failure is `Rir::Zero`, which is the reliable signal anyway. `failure` was used
inconsistently — sometimes a prescription, sometimes an observation — and
abandoned: 6 uses in 2024, 70 in 2025, **1 in 1,335 sets in 2026**, against 461
sets at RPE 10 of which only 67 carry the flag. An unrecognised kind fails
translation rather than defaulting.

**A superset is exercises performed back to back.** That is the definition, so
its members are contiguous and there are at least two of them, and a container
in the ordered sequence is the type that says so.

Translation cannot repair a record that breaks this, because every repair is a
guess. The workout translates, the ungrammatical grouping does not, and the
omission is recorded (§ 37). It is the first concrete case for the edit overlay.

Grouping *sets* rather than exercises was considered and is arguably more
faithful to what happens, but it would require exercise identity to move onto
the set. Rejected on that basis.

**A session sits above the workout.** The source's workout boundary is not the
session boundary. 2025-10-06 and 2025-10-10 each landed four back-to-back
records — one training session, fragmented by an attempt to make workout parts
reusable — and 21 days carry more than one record. Without the container, every
session count, frequency figure and streak over those days is inflated (§ 10).

§ 4 licenses it: a canonical entity "names the normalised entities it stands
for", plural. Composition is structure at the canonical layer, not a
correspondence, so § 10's supersession and co-observation cases are silent on it
rather than contradicted — the first governs records *sharing a source
identity*, which four distinct workouts do not.

**Identity is one level: the exercise.** A set belongs to an exercise, and that
is the whole of it.

The alternative was two levels — a movement with variants, so `Front Squat` is
the `Front` variant of `Squat`. It fails because variants are not independent of
movements: `Front`, `Back` and `Zercher` are squat variants and mean nothing
applied to a pull-up, so a shared variant vocabulary makes illegal pairs
constructible (§ 24), and a per-movement one needs a type per movement. It also
forces answers nothing yet consumes — whether `Thruster` is a squat or a press,
whether `Snatch Balance` is a squat — with no consumer to check them against.

Grouping arrives later as a relation over exercises rather than a level inside
them (open question 4). A relation only asserts what is true, so nothing illegal
is representable, and `Front Squat` can belong to a squat group and a front-rack
group at once — which a hierarchy cannot express, since it forces one parent.

**Identity maps from the source's identifier, not its labels.** Neither of
Hevy's labels is stable: `Overhead Squat` has two `exercise_template_id`s, a
builtin and a custom, and template `DDB29047` has appeared under two titles,
having been renamed mid-history. 26 of 134 templates are custom.

So the mapping is a version-controlled table keyed on template id, many-to-one
onto our exercises, with titles informing it and never keying it — § 8's
"deterministic translation merges where a source over-separates", which is the
same mechanism that collapses assisted and unassisted pull-ups. Per id it
resolves the exercise and its load interpretation. It is code, not data (§ 9),
and an unmapped id fails loudly.

A source's declared type informs that mapping without determining it. `Pull Up`
is `reps_only` in Hevy and `Pull Up (Assisted)` is `bodyweight_assisted_reps`,
yet they are one exercise here.

**Implement and laterality are not fields.** "Implement" was doing two unrelated
jobs — per-limb reading and resistance kind — and its bodyweight case duplicated
what `Relative` already says. Trap bar is a deadlift variant; suitcase carry is
the single-arm variant of farmer's carry. Naming absorbs these; an attribute
schema would not have absorbed the safety bar or Zercher case.

**"Exercise" is the right word for the container.** The literature spends
"exercise" on the movement and has no term for the container, because
programming compresses (`back squat 3×5 @ 80%`) in a way recording cannot —
each performed set has its own reps, RIR and rest. The gap is real and inventing
a term is legitimate, but an exercise holding its sets will not mislead anyone.

**Rest is a fact even where nothing records it.** Resting two minutes rather
than three between sets is a signal of progress, so `rest_after` belongs on the
performed set. Hevy supplies none — its logged Set carries no rest field and no
per-set timestamps — and reconstructing it from a linked routine would mean
assuming every set took its prescribed rest, which is prescription masquerading
as observation (§ 11). So it is optional, and permanently absent from this one
adapter, which is § 37 working rather than a gap in the model.

**Load carries sub-kilo precision, so a float is the wrong carrier.** Weights in
the corpus include `.1`, `.2` and `.4`, hand-converted from pound-denominated
machines. Fixed point, because the value is persisted, digested and compared
against rows written by earlier versions (§ 7).

---

## Checked against the data

The model was written out as rules and every landed record run through it.
**3,755 of 3,779 sets translate completely**, along with 334 of 336 supersets.
What did not fit falls into three kinds, and telling them apart is the point of
the exercise: a model that cannot hold a genuine case needs refining, whereas a
model that rejects a wrong record is working.

| | Sets | Verdict |
|---|---:|---|
| Zero load on an absolute-load exercise | 7 | Wrong data |
| Band resistance | 16 | Known limitation, accepted |
| A set of zero reps | 1 | Genuine case, not modelled |
| Non-contiguous superset | — | Wrong data |
| Single-member superset | — | Wrong data |

**The seven zeros are all the bottom of a warm-up ramp** — `0, 5, 10` on a good
morning; `0, 15, 20` on an overhead squat; `0, 0` then `105, 105, 105` on a
Romanian deadlift, the zeros tagged `warmup`. They mean "empty bar" or "PVC
pipe", which is real technique work recorded lossily, since the bar's own mass
went unwritten and 10, 15 and 20 kg bars are all in use. The event is genuine
and the recording is wrong; it is fixed at source or in the edit overlay, not by
weakening the type.

**The two malformed supersets are malformed.** One has members either side of a
non-member; one has a single member, the last exercise in the workout, where the
partner was never added. Both fail the definition rather than testing it.

**The one genuine gap is the zero-rep set**: 95 kg × 0 reps at `Rir::Zero`, an
attempt that failed. It is a real event and it is not a set, so no refinement of
`RepCount` captures it honestly — it needs an *attempt*, which belongs with
prescribed-versus-performed (open question 3).

Two things the run settled that argument had not:

**Load applicability is nearly a non-question.** Bodyweight movements are
`Relative(0)`, which covers running, skipping, stretching and mobility work.
That leaves two exercises where resistance applies, is not bodyweight, and was
never captured — the air bike's fan and the sled's load, 41 sets between them.
Small enough to defer, and named in open question 5 rather than paid for with a
variant on `Load`.

**The session rule is not an arbitrary threshold.** Of 27 consecutive same-day
workout pairs, the largest gap between one ending and the next starting is
**8.3 minutes**, and twelve of them are under ten seconds. Nothing sits between
that and the following day, so any threshold from ten minutes to several hours
gives the same answer. The corpus resolves to **136 sessions** from 163 workout
records.

Nothing in the run exercises supersession, deletion or timezone handling: the
corpus holds no re-serve to test against.

---

## Prescribed vs performed

§ 8 requires the split; it is load-bearing rather than tidy.

- A prescribed set takes a rep *range* and a rest *instruction*. A performed set
  takes actuals. You cannot perform a range.
- Prescription compresses (3×5 @ 80% → sets not enumerated because they don't
  exist yet); performance must enumerate.

The corpus shows what happens without the split. Hevy has no prescribed side, so
intent leaks into observation records wherever it finds a field: the `failure`
set kind, exercise notes carrying "do lengthened partials when you can't do full
reps", and the missed single above, which had nowhere else to go.

Hevy's own prescribed side is not a substitute. `routine_id` is populated on 8
of 163 workouts, and the routines it names have since been deleted. We will own
the prescription.

---

## What the Hevy adapter has to deal with

Verified against the pinned OpenAPI spec and the landed corpus. None of this
shapes the model above; it is what one adapter must translate into it.

- **No rest field, no per-set timestamps.** `rest_after` is always absent.
- **RPE only**, on eight positions: `6, 7, 7.5, 8, 8.5, 9, 9.5, 10`.
- **No band or assistance concept.** Assisted movements are separately-named
  exercises carrying positive weight, requiring negation at translation.
- **The sign convention is invisible in a workout payload.** `weight_reps`,
  `bodyweight_reps` and `bodyweight_assisted_reps` serialise identically;
  `ExerciseTemplate.type` is the only published source of it.
- **`start_time` is a true UTC instant**, not a naive wall clock stamped `Z`:
  starts cluster at 18:00 UTC through BST and 19:00–20:00 UTC through GMT, a
  clean one-hour shift. § II.3's "zone from declared operator configuration" is
  the correct treatment, and travel is invisible in the payload — an edit-overlay
  correction, as § II.3 anticipates.
- **No identity below the workout.** Sets and exercises carry only a positional
  index, which moves under insertion or reordering. See open question 6.
- **Zero is overloaded.** 93 sets carry a zero load. 58 are legitimate under the
  performable-zero rule; 28 more resolve once bodyweight movements are read as
  `Relative`; 7 are wrong. Nine `Sled Push` sets record a zero distance the same
  way.
- **The feed is current-state, not an event log.** A deletion replaces a
  workout's row rather than adding one, so a workout created and deleted between
  two runs arrives as a body-less tombstone — one is already landed. Detail in
  [`specs/001-hevy-workout-extraction/research.md`](../specs/001-hevy-workout-extraction/research.md).
- **Supersession has no ground truth.** 164 records, 164 distinct workout ids,
  not one re-serve. § 10's "the later supersedes" needs a synthetic test.

Other sources: BTWB has a self-service CSV export. PushPress has no documented
member-facing export; a Playwright-scraping CLI exists and was rejected as an
adapter basis. Spreadsheets are the original source of this set model, so
expected to fit rather than strain; units may be implicit or inconsistent per
row.

---

## Prior art reviewed

- **openweight** (openweight.dev) — closest analogue: vendor-neutral JSON for
  strength training. Workout → Exercise → Set; set is reps, weight, unit, rpe.
  Flat, variant in the name, no assistance or band concept. Its principles
  restate § II.2 and § 6 almost exactly. Diverges on units: it carries `unit`
  per set because an interchange format must preserve what the source said,
  whereas an analytical store normalises so series compare.
- **Open exercise datasets** (free-exercise-db, wrkout/exercises.json,
  ExerciseDB) — flat catalogues with equipment/force/mechanic/muscle attributes.
  Built for exercise *pickers*, not longitudinal comparability. None attempt the
  assisted-to-unassisted collapse.
- **OpenSet** (openset.dev) — **not read.** Claims 10 execution modes and 21
  composable dimensions spanning strength, endurance and conditioning. Most
  likely place AMRAP/EMOM is modelled structurally, and the only standard seen
  spanning strength *and* cardio.
- **Sports-science literature** (NSCA-derived) — a set is a group of repetitions
  performed consecutively before stopping to rest; the rest boundary *is* the
  set boundary. Exercise means the movement. Superset = two exercises, different
  or opposing muscle groups; compound set = same muscle group. No term for the
  container, and no term for what an exercise is measured in, because it assumes
  reps and load.

---

## Open questions

1. **Is a day a container?** Relative strength needs a bodyweight to divide by,
   and body measurements have no parent to join on. The case against making the
   day an entity is that the join rule — same calendar day, nearest reading,
   most recent before — is itself a modelling choice, and § 6 requires a derived
   series to declare its method including that choice. A container picks one
   silently, and leaves a day without a measurement wanting a value it does not
   have (§ 37).
2. **Does `Distance` carry an optional duration, or are carries and runs
   different measures?** Optional duration says a 20 m farmer's walk and a 200 m
   run are one measure recorded to different depths. Separate measures say one
   is time under load and the other is pace, and they do not belong on one axis.
3. **Attempts.** A rep attempted and missed is a real event and not a set. One
   occurrence so far. Belongs with prescribed-versus-performed, since a source
   with no prescribed side leaves nowhere to record intent that went unmet.
4. **The grouping layer** — "all squatting volume", "all pull-up variants". A
   relation over exercises, many-to-many, added without unpicking identity. Not
   designed.
5. **Resistance that is neither bodyweight nor recorded** — the air bike's fan,
   the sled's load. 41 sets across two exercises. Either those exercises leave
   the entity, or the vocabulary distinguishes load-applicable from not.
6. **Overlay anchors below the workout.** § II.2 requires overlays anchor to
   source identity, and Hevy publishes none below the workout, so the obligation
   is unsatisfiable as written. An anchor derived from content — exercise,
   ordinal, recorded values — survives a rebuild, which is the rule's stated
   purpose, and lapses when the set changes at source, which is § II.2's own
   "overrides do not propagate" one level down. Reads as a violation until that
   is noticed; settled when the overlay is built.
7. **Composition in the match overlay.** The overlay's vocabulary is "the same
   real-world event", which does not obviously express "is part of". To be
   settled against a concrete case.
8. **Machine identity.** A weight stack is not comparable across machines — the
   corpus carries a note reading "TechnoGym single purpose leg extension
   (settings: 2-2-max)" for that reason — but the notes are inconsistent and
   nothing else records it. Not a § 6 gap: method-dependence there concerns
   sensors and algorithms, and a stack is neither. An § 8 identity question,
   standing as a declared limitation.
9. **Per-limb load** — whether two 15kg dumbbells records 15 or 30. Resolved by
   naming, but historical recording is expected to be inconsistent. An
   edit-overlay problem, not a modelling one.
10. **Block-level results** for AMRAP/EMOM — a score attributable to no set.
    Three workouts repeat an exercise entry, most likely rounds encoded without
    supersets, which is evidence but not an answer. Blocked on a BTWB export.
11. **Cluster sets** — intra-set rest. Probably separate sets with a short
    `rest_after`, per the rest-defined set boundary, but nothing confirms it.
12. **Logging as a first-class capability** — "build my own Hevy". Would make
    the platform a system of record for data no source produces, which § II.2
    does not currently accommodate. Architectural change, not a feature.
    Motivated by the rest-recording hole.

---

## Not modelled here

- The other five entities.
- The prescribed set beyond noting how it differs.
- The exercise vocabulary itself — the enums above are illustrative, and their
  members come from the mapping exercise.
