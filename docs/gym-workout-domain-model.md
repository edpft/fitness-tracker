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
    Absolute(Kg),         // external load; 0 = none, which is a real answer
    Relative(SignedKg),   // delta against bodyweight; 0 = plain bodyweight
}

struct Distance { metres: Metres }

/// Reps in reserve. An ordinal scale: positions order and compare, they do not
/// average or subtract.
enum Rir {
    Zero, ZeroOrOne, One, OneOrTwo, Two, TwoOrThree, Three, FourOrMore,
}

enum SetKind { Working, Warmup }

/// What became of a set. A completed one carries its measure; a failed attempt
/// carries none, so no arithmetic can take a quantity from a failure.
enum Performed<M> { Completed(M), Failed }

struct Set<M> {
    load: Load,
    /// Not `measure: M`. The load is outside the outcome, because a failed
    /// attempt is a load that was on the bar.
    outcome: Performed<M>,
    intensity: Option<Rir>,
    kind: SetKind,
    rest_after: Option<Duration>,
}

/// The exercise vocabulary, partitioned by what its members are measured in.
enum RepsExercise     { FrontSquat, Deadlift, PullUp, ChestDip, BoxJump, /* … */ }
enum DurationExercise { DeadHang, HandstandHold, Stretching, SledPush, /* … */ }
enum DistanceExercise { Running, FarmersWalk, WalkingLunge, /* … */ }

enum PerformedExercise {
    ForReps     { exercise: RepsExercise,     sets: NonEmpty<Set<RepCount>> },
    ForDuration { exercise: DurationExercise, sets: NonEmpty<Set<Duration>> },
    ForDistance { exercise: DistanceExercise, sets: NonEmpty<Set<Distance>> },
}

struct Superset { members: AtLeastTwo<PerformedExercise> }  // two or more, back to back

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

**Absolute vs Relative is decided by whether the load axis runs both ways.** A
load is `Relative` where assistance is conventionally available as well as added
weight — a pull-up, a chin-up, a dip. The bodyweight version is the movement,
machines and bands routinely make it easier, and a belt routinely makes it
harder, so the axis passes through zero and the sign carries meaning. It is
`Absolute` where only adding is a thing anyone does — a squat, a deadlift — so
the number is external load and none of it is a real answer.

This is a convention rather than a physical fact, which is why it is decided per
exercise in the mapping and never inferred from a value. It is also what keeps a
100kg squat and a bodyweight dip +10kg from being read as the same kind of
number.

An earlier version of this rule asked whether zero was *performable*, and made a
zero on a barbell exercise an error by construction. That diagnosed the empty-bar
warm-ups below correctly by accident: what makes them look wrong is that a
barbell has mass, which is a fact about the implement rather than about the
direction of the axis. [Decision
0004](decisions/0004-the-load-axis-is-bidirectional-or-it-is-not.md).

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

**Band tension is recorded as given, and what it means is declared.** Tension
varies through the range of motion, so no scalar is the whole truth — but the
number that was written down is the operator's estimate of the band, and
discarding it forecloses an overlay supplying a resistance range later. So a
banded lateral raise reads its number as external load, and a banded pull-up
negates it onto the relative axis like any other assistance.

What stays a declared limitation is comparability. The account's assisted loads
run `0, 7, 14, 21, 28, 35, 42` — stacked bands rather than a machine stack — and
deterministic translation (§ 9) cannot tell the two apart, so band and machine
assistance are not the same series even though they sit on the same axis.

**Three measures, because there are three things you can count.** Repetitions,
elapsed time, and ground covered. Everything else a source offers is one of
those recorded more or less fully.

A fourth — ground covered in a time — was added and then removed. Every entry
that would have used it repeats one identical distance and duration across all
of its sets (`400m/150s ×3`, `200m/60s ×5`), and identical across the entry is
the signature of a target rather than a measurement. It was an interval
prescription with nowhere else to go, which is § 11 working exactly as this
document predicts it will when the prescribed side is missing. [Decision
0005](decisions/0005-distance-over-time-was-prescription.md).

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

*Those were the figures on paper. Building the rules and then reviewing them
moved both — see "What building it changed" below. The short version is that
3,778 of 3,779 sets translate and the one that does not is the missed attempt.*
What did not fit falls into three kinds, and telling them apart is the point of
the exercise: a model that cannot hold a genuine case needs refining, whereas a
model that rejects a wrong record is working.

| | Sets | Verdict on paper | Verdict now |
|---|---:|---|---|
| Zero load on an absolute-load exercise | 7 | Wrong data | Translates — no external load ([0004](decisions/0004-the-load-axis-is-bidirectional-or-it-is-not.md)) |
| Band resistance | 16 | Known limitation, accepted | Translates — the number is read as load ([0004](decisions/0004-the-load-axis-is-bidirectional-or-it-is-not.md)) |
| A set of zero reps | 1 | Genuine case, not modelled | Unchanged |
| Non-contiguous superset | — | Wrong data | Unchanged |
| Single-member superset | — | Wrong data | Unchanged |

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

**Load applicability is nearly a non-question.** Movements with nothing on them
are `Absolute(0)` — no external load — which covers running, skipping,
stretching and mobility work. That leaves the sled: it has plates on it, the
number is not recorded, and `Absolute(0)` therefore understates it. Nine sets, a
declared limitation, and not paid for with a variant on `Load`.

**The session rule is not an arbitrary threshold.** Of 27 consecutive same-day
workout pairs, the largest gap between one ending and the next starting is
**8.3 minutes**, and twelve of them are under ten seconds. Nothing sits between
that and the following day, so any threshold from ten minutes to several hours
gives the same answer. The corpus resolves to **136 sessions** from 163 workout
records.

Nothing in the paper run exercises supersession, deletion or timezone handling:
the corpus holds no re-serve to test against.

## What building it changed

The rules above were written out and applied by hand, then built, then reviewed.
Both of the later steps moved figures, and in every case the paper run had
stopped a level too early rather than the rules being wrong about training.

**Building them** showed that a refusal does not only remove a set: it can cost
the set's *entry*, and a lost entry can cost its *grouping* the second member
that made it a superset. On the paper figures that turned 1,135 entries into
1,122 and 334 supersets into 328.

**Reviewing them** removed the refusals those cascades came from. With
`Absolute` admitting zero ([0004]) and band resistance read as load, nothing
refuses except the missed attempt and the two malformed groupings — so the
cascades have nothing to cascade from.

| | On paper | Built |
|---|---:|---:|
| Sets translating | 3,755 of 3,779 | **3,778** |
| Exercise entries | 1,135 | 1,135 |
| Supersets | 334 of 336 | 334 |
| Warm-up sets | 361 | 361 |
| Sets carrying intensity | 2,415 | **2,414** |
| Refusals | 26 | **3** |

The one set that does not translate is the missed attempt, and the two groupings
that do not are the two that fail the definition of a superset. A model that
rejects exactly the things it has no shape for is a better result than one that
rejects twenty-six things for five different reasons — and getting there meant
giving up a rule that had been diagnosing seven records correctly for the wrong
reason.

[0004]: decisions/0004-the-load-axis-is-bidirectional-or-it-is-not.md

## Known aliases, for the edit overlay

Templates the operator has used to stand for a different movement. Deterministic
translation cannot see any of this — the template does not determine what was
performed — so the mapping does not pretend to, and these wait for the overlay.

| Template | Has also meant |
|---|---|
| `Cable Twist (Up to down)` | A bent-over cable chop |
| `Inverted Row`, `Low Row (Suspension)` | Ring rows — the builtin exercises carry a picture and metadata that a custom one does not, so they get used in preference |
| `Seated Palms Up Wrist Curl` | A generic dumbbell wrist flexion |
| `Seated Wrist Extension (Barbell)`, `Reverse Wrist Curl (Dumbbell)` | A generic dumbbell wrist extension |
| `Triceps Extension (Cable)` | The overhead variant |
| `Stretching` | A deep squat stretch |

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
  two runs arrives as a body-less tombstone — one is already landed, carrying an
  id and a `deleted_at` and nothing else. It is a retraction, and it leaves the
  workout it names with no normalised entity: [decision
  0001](decisions/0001-retraction-at-the-normalised-layer.md), which amended the
  constitution to say so. Detail on the feed in
  `specs/001-hevy-workout-extraction/research.md`, deleted 2026-09-01 and
  recoverable from git history.
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
2. ~~**Does `Distance` carry an optional duration, or are carries and runs
   different measures?**~~ **Settled**: neither. There is one distance measure
   and no duration on it, because the duration was a target rather than an
   observation. [Decision
   0005](decisions/0005-distance-over-time-was-prescription.md).
3. ~~**Attempts.** A rep attempted and missed is a real event and not a set.~~
   **Settled**: `Set<M>` holds `outcome: Performed<M>`, so a failed attempt is a
   set with a load and no measure. It was right that this belonged with
   prescribed-versus-performed — the negative gate in
   `primary-lift-progression.md` detects a stall from a miss, which is what gave
   the case something to mean — and decision
   [0007](decisions/0007-a-zero-rep-set-is-a-failed-attempt.md) records the
   reversal. `RepCount` stays non-zero; the zero is a sentinel read in
   translation, and Hevy's `failure` set *type* is deliberately not the
   discriminator: it sits on 77 sets and means "taken to failure".

   **One thing this uncovered.** A failed attempt carries no *intended* count
   either, because nothing records what was being attempted, so the round trip in
   `prescribed-workout-domain-model.md` reports it as a gap rather than guessing.
   The performed model still cannot fully describe a missed set, and that is a
   smaller open question than this one was.
4. **The grouping layer** — "all squatting volume", "all pull-up variants". A
   relation over exercises, many-to-many, added without unpicking identity. Not
   designed.
5. ~~**Resistance that is neither bodyweight nor recorded**~~ — **Settled**, and
   mostly dissolved. An air bike carries no external load, and `Absolute(0)`
   says exactly that. The sled does carry one and it is not recorded, so nine
   sets understate what was moved: one declared limitation rather than two.
   [Decision
   0004](decisions/0004-the-load-axis-is-bidirectional-or-it-is-not.md).
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
