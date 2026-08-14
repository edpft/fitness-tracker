# Gym Workout Domain Model — Working Record

*Where the reasoning behind the gym-workout entity lives. Companion to the
constitution; nothing here overrides it. Not ratified — this is not a decision
record, and the Rust below is a sketch rather than an interface.*

*Modelled bottom-up from the Set, then tested against the landed Hevy corpus:
164 records — 163 `updated`, 1 `deleted` — covering November 2024 to August 2026,
1,135 exercise entries, 3,779 sets, 134 distinct exercise templates. Counts
quoted throughout are from that corpus.*

---

## Entity inventory

Six entities. One is modelled here; the rest are named so the boundaries are
visible.

| Entity | Source | Shape |
|---|---|---|
| **Gym workout** | Hevy | Session → workouts → exercises → sets. |
| **Cycling workout** | Peloton | Summary + per-second sample stream. |
| **Body measurement** | Withings | One event, many metrics, one instant. Flat. |
| **Sleep** | Garmin | An interval crossing a date boundary, with stage durations. |
| **Daily physiological reading** | Garmin | HRV, RHR. One value per day, no parent session. |
| **Nutrition** | MacroFactor | A day's intake, plus a modelled expenditure estimate. |

- Body measurement and daily reading have no parent — they are § II.2's
  "degenerate entity".
- Nutrition's expenditure estimate is modelled, not measured, and may not belong
  in the same entity as intake.
- Sleep is the first entity that breaks the wall-clock timestamp assumption
  cleanly: a night is an interval, not an instant.

**It is a gym workout, not a strength workout.** 208 sets in the corpus are
running, skipping, sled work, stretching and isometric holds. The entity's
boundary is the venue and the recording style — a session logged as exercises
and sets — in opposition to an outdoor ride, a swim or a Peloton class. A
warm-up on a stationary bike inside a lifting session belongs here; the same
bike ride recorded by Garmin as its own activity does not.

---

## The types

```rust
struct Kg(Grams);        // non-negative, fixed point
struct SignedKg(Grams);  // negative = assistance

enum Load {
    Absolute(Kg),        // barbell, dumbbell, machine stack — the load is the whole load
    Relative(SignedKg),  // delta against bodyweight; 0 = plain bodyweight
}

/// What one set records. The variant is fixed by the exercise, so a set cannot
/// carry a shape its exercise does not have.
enum Effort {
    Reps(RepCount),
    LoadedReps { load: Load, reps: RepCount },
    Duration(Duration),
    LoadedDuration { load: Load, duration: Duration },
    DistanceOverTime { distance: Metres, duration: Duration },
    LoadedDistance { load: Load, distance: Metres },
}

/// Reps in reserve. Half-points are uncertainty, not fractions.
enum Rir {
    Exactly(ReserveCount),
    Between(ReserveCount, ReserveCount),
    AtLeast(ReserveCount),
}

/// What the source called this set. Source vocabulary, deliberately not
/// interpreted — see "Set kind is carried, never read" below.
enum RecordedSetKind {
    Normal,
    Warmup,
    Failure,
    Dropset,
    Unrecognised(RawSetKind),
}

struct PerformedSet {
    effort: Effort,
    intensity: Option<Rir>,
    kind: RecordedSetKind,
}
```

Above the set:

- **Exercise** — a movement reference, a non-empty ordered list of sets all of
  one `Effort` variant, and an optional superset membership.
- **Workout** — a non-empty ordered list of exercises. Superset grouping is an
  attribute of the exercises, not a container over them.
- **Session** — a non-empty ordered list of workouts. Canonical layer.

Identity is **Movement → Variant**. A squat is a movement; front, low-bar,
Zercher are variants. The exercise-in-a-workout references a Variant. Prose
only, deliberately: the concrete form is feature work.

There is no `rest_after` on a performed set. Hevy's logged Set carries no rest
field and no per-set timestamps, so actual rest is permanently unrecoverable
from history. It belongs on the prescribed set, where it is an instruction.

---

## Why each decision went the way it did

**Load is a sum type, and bands are not modelled.** Band tension varies through
the range of motion, so no scalar is honest — but Hevy has no band concept, and
`BandId` is not constructible from anything it serves. Assistance arrives as a
bare number with no mechanism attached, and the account's assisted loads run
`0, 7, 14, 21, 28, 35, 42` — stacked bands, not a machine stack. Deterministic
translation (§ 9) cannot tell band from machine, so a `Band` field would be a
type nothing could populate.

The nine band-named sets (`Pull Up (Band)` at 14 kg, `Front Raise (Band)` and
`Lateral Raise (Band)` at 5 kg) carry operator estimates — a guess at the gym's
band, or a manufacturer's advertised range — rather than measurements. Raw
retains them (§ II.1) whatever the domain does. Band-assisted and
machine-assisted work therefore both normalise to `Relative(−n)`, and the fact
that they are not comparable under § 4 is a limitation to declare rather than
one the value can express.

**Absolute vs Relative, not external vs added.** A 100kg squat and a bodyweight
dip +10kg are not the same fact: one number is the total, the other a delta
against a bodyweight the set doesn't record. Splitting on "is this the whole
load or a delta" keeps a consumer from needing to know which exercise it is to
read the number.

**Assistance is negative, not a separate case.** Assisted −20 and weighted +10
sit on one axis, and the crossover through zero is a genuine progression that
must not change type. This is the single motivating case for the whole load
model — collapsing "unassisted pull-up" and "pull-up with 0kg assistance" into
one series — and the corpus is full of it: `Pull Up` (97 sets, no load),
`Pull Up (Assisted)` (159), `Chest Dip` (84), `Chest Dip (Assisted)` (277).

**Effort is one axis, not two independent ones.** Every one of the 134 templates
uses exactly one field signature across its whole history — no exceptions —
and five signatures occur:

| Wire shape | Sets | Templates |
|---|---:|---:|
| `weight_kg, reps` | 2,969 | 91 |
| `reps` | 602 | 32 |
| `duration_seconds` | 139 | 7 |
| `weight_kg, distance_meters` | 41 | 2 |
| `distance_meters, duration_seconds` | 28 | 2 |

So a measure-only sum type is wrong in one direction: 19 `Running` sets record a
distance *and* the time it took, which it cannot hold. Four independent optional
fields, mirroring the wire format, are wrong in the other: they admit
reps-and-distance, which never occurs. Binding the shape to the exercise admits
exactly what happens (§ 24), and it is the exercise rather than the set because
a template's shape never varies between its sets.

The variants cover Hevy's eight declared exercise types, with the load
convention carried by `Load` rather than by a variant of its own —
`weight_reps`, `bodyweight_reps` (+kg) and `bodyweight_assisted_reps` (−kg) all
become `LoadedReps`, differing in whether the load is `Absolute` or `Relative`.
Which of the three applies is invisible in a workout payload; it comes from the
exercise mapping below.

**Intensity is RIR, and the half-points are uncertainty.** Hevy's scale is
`6, 7, 7.5, 8, 8.5, 9, 9.5, 10`, glossed in its own UI as reps in reserve
(10 = 0, 9.5 = maybe 1, 9 = 1, 8.5 = maybe 2 …) and used that way, which makes
RIR the recorded fact and RPE the transport's spelling of it. 984 of the 2,415
recorded intensities are half-points, so an integer type would lose 40% of them,
and a float would imply a precision the scale does not have — 9.25 is not
sayable. `Between` says what a half-point means.

`AtLeast` exists because 6 is the floor of Hevy's scale, not a measurement of
four: everything easier than four in reserve is recorded as 6 or left blank.
Coverage is 64% of sets and stable across all three years.

Modelling RIR rather than Hevy's eight-value lattice keeps a transport's shape
out of the domain, where a source recording exact RIR would not fit.

**Set kind is carried, never read.** Warm-up/working, top/back-off, drop and
failure are unrelated axes, not one enum — `Working` and `TopSet` would be
falsely exclusive — and Hevy conflates several of them into one field. Carrying
that field verbatim, and reading none of it as a domain fact, is what respects
both.

Carrying it is not optional. "Raw retains it" does not survive the layering:
§ 5 builds analysis over canonical, § 4 over normalised, so a fact dropped at
translation is unreachable without reaching around the layers. Volume metrics
need warm-up exclusion, and 361 sets are warm-ups. No positional rule
reconstructs them either — the corpus opens workouts with heavy bridging singles
tagged `warmup`.

Reading it is a mistake. `failure` was used inconsistently — sometimes a
prescription (*take this to failure*), sometimes an observation (*this ended in
failure*) — and abandoned: 6 uses in 2024, 70 in 2025, **1 in 1,335 sets in
2026**. Meanwhile 461 sets carry RPE 10 and only 67 of them are marked
`failure`. RIR 0 is the consistent signal for a set taken to failure; the flag
is the noisy one, and it is § 11 being violated inside the source record. Any
field that can carry either intent lands as source text and stays there — which
covers exercise notes too, where "Tempo: 2-1-1" (performance) and "do lengthened
partials when you can't do full reps" (prescription) share one field.

`Dropset` occurs zero times in the corpus and is in Hevy's documented
vocabulary, so the type admits it. `Unrecognised` is there because the
vocabulary is theirs to extend.

**Superset membership is an attribute, not a container.** Hevy indicates
membership by colour and does not constrain ordering, so members need not be
adjacent: on 2025-03-31 one group holds exercises 3 (`Running`) and 5 (`V Up`),
with a non-member at index 4 between them. An ordered list of items, where an
item is either an exercise or a superset, cannot represent that — a container
occupying a position forces its members together. 122 of 163 workouts use
supersets, so this is load-bearing structure rather than a corner.

Round structure stays implicit in set ordering, and order within a group is a
fact. Hevy's `superset_id` is a sparse workout-scoped integer, reused across
workouts — one workout carries groups 0, 1, 3 and 5 — so it must not cross into
the domain as an identity.

Grouping *sets* rather than exercises was considered and is arguably more
faithful to what happens, but it would require exercise identity to move onto
the set. Rejected on that basis.

**A session sits above the workout.** The source's workout boundary is not the
session boundary. 2025-10-06 and 2025-10-10 each landed four back-to-back
records 7–20 minutes apart (`Snatch Progression`, `Vertical Pull & Push`,
`Front Squats`, `Horizontal Pull & Push`) — one training session, fragmented by
an attempt to make workout parts reusable. 21 days carry more than one record.
Without the container, every session count, frequency figure and streak over
those days is inflated (§ 10).

§ 4 licenses it directly: a canonical entity "names the normalised entities it
stands for", plural. The composition is structure at the canonical layer, not a
correspondence, so § 10's supersession/co-observation dichotomy is silent on it
rather than contradicted — the first bullet governs records *sharing a source
identity*, which four distinct workouts do not.

The container is not gym-specific, which is the point: historical Garmin data
carries bike-and-run triathlon sessions of the same shape, and a session
composed from two sources is what § 4 exists for.

**Identity maps from the source's template id.** Neither of Hevy's labels is
stable. `Overhead Squat` has two `exercise_template_id`s — a builtin and a
user-created custom — and template `DDB29047` appears under two titles, having
been renamed mid-history. 26 of 134 templates are custom.

So the mapping is a version-controlled table keyed on template id, many-to-one
onto domain exercises, with titles informing it and never keying it. That is
§ 8 working as intended: deterministic translation merges where a source
over-separates, which is the same mechanism that collapses assisted and
unassisted pull-ups. It declares four things per id — domain exercise, load
interpretation, effort shape, and whether a zero load is meaningful — and it is
code, not data (§ 9). An unmapped id fails loudly rather than passing through.

**Implement and laterality are not fields.** Considered, then rejected:
"implement" was doing two unrelated jobs (per-limb reading vs resistance kind),
and its bodyweight case duplicated what `Relative` already says. Trap bar is a
deadlift variant; suitcase carry is the single-arm variant of farmer's carry.
Naming absorbs these; an attribute schema would not have absorbed the safety bar
or Zercher case.

**Movement → Variant for identity.** The variant is where § 4's series boundary
lands. Grip, bar position and implement are variant-distinguishing because you
never *progress along* them — unlike load, which is why load is structural and
these are not. Grouping ("all squatting volume") is a layer over identity, not
an attribute of it.

The case that strains this is the pull-up, which varies by grip *and* assistance
mode independently. Assistance living in `Load` rather than in the name
collapses the variant list back to grip alone.

**"Exercise" is the right word for the container.** Sports-science literature
spends "exercise" on the movement and has no term for the container, because
programming compresses (`back squat 3×5 @ 80%`) in a way recording cannot —
each performed set has its own reps, RIR and rest. The vocabulary gap is real
and inventing a term is legitimate, but `Exercise { movement, sets }` won't
mislead anyone.

**Load carries sub-kilo precision, and a float is the wrong carrier.** Weights
in the corpus include `.1`, `.2` and `.4` — hand-converted from pound-denominated
machines. Fixed point, because the value is persisted, digested and compared
against rows written by earlier versions (§ 7).

---

## Prescribed vs performed

§ 8 requires the split; it is load-bearing rather than tidy.

- `PrescribedSet` takes a rep *range* and a rest *instruction*. `PerformedSet`
  takes actuals. You cannot perform a range.
- Prescription compresses (3×5 @ 80% → sets not enumerated because they don't
  exist yet); performance must enumerate.

The corpus shows what happens without the split. Hevy has no prescribed side, so
intent leaks into observation records wherever it can find a field: the
`failure` set kind, exercise notes, and one set recording 95 kg × **0 reps** at
RPE 10 — a single that was attempted and missed, with nowhere else to say so.

Hevy's own prescribed side is not a substitute. `routine_id` is populated on 8
of 163 workouts, and the routines it names have since been deleted. We will own
the prescription.

---

## What Hevy actually serves

Verified against the pinned OpenAPI spec and the landed corpus.

- **No rest field, no per-set timestamps.** Actual rest is unrecoverable.
- **RPE only**, on the eight-value lattice above.
- **No band or assistance concept.** Assisted movements are separately-named
  exercises carrying positive weight, requiring negation at translation.
- **The sign convention is invisible in a workout payload.** `weight_reps`,
  `bodyweight_reps` and `bodyweight_assisted_reps` serialise identically.
  `ExerciseTemplate.type` is the only published source of it.
- `supersets_id` sits on Exercise, confirming exercise-level grouping.
- **`start_time` is a true UTC instant**, not a naive wall clock stamped `Z`:
  starts cluster at 18:00 UTC through BST and 19:00–20:00 UTC through GMT, a
  clean one-hour shift. § II.3's "zone from declared operator configuration" is
  the correct treatment, and travel is invisible in the payload — an edit-overlay
  correction, as § II.3 anticipates.
- **No identity below the workout.** Sets and exercises carry only a positional
  `index`, which moves under insertion or reordering.
- **Zero is overloaded.** 93 sets carry a zero load: 58 legitimately, on
  `(Assisted)` and `(Weighted)` templates where zero means unassisted or no
  added load; 35 not, spread over 9 templates where it is a data-entry hole —
  `Romanian Deadlift (Barbell)` at 0 kg is not a lift, and the bar's own mass is
  unknowable (10, 15 and 20 kg bars are all in use). Those are exclusions, not
  translations. Nine `Sled Push` sets record a zero distance the same way.
- **The feed is current-state, not an event log.** A deletion replaces a
  workout's row rather than adding one, so a workout created and deleted between
  two runs arrives as a body-less tombstone — one such record is already landed.
  Detail in
  [`specs/001-hevy-workout-extraction/research.md`](../specs/001-hevy-workout-extraction/research.md).
- **Supersession has no ground truth.** 164 records, 164 distinct workout ids,
  not one re-serve. § 10's "the later supersedes" needs a synthetic test.

Other sources: BTWB has a self-service CSV export (account access was a blocker
when this was written). PushPress has no documented member-facing export; a
Playwright-scraping CLI exists and was rejected as an adapter basis. Spreadsheets
are the original source of this set model, so expected to fit rather than strain;
units may be implicit or inconsistent per row.

---

## Prior art reviewed

- **openweight** (openweight.dev) — closest analogue: vendor-neutral JSON for
  strength training. Workout → Exercise → Set; set is reps, weight, unit, rpe.
  Flat, variant in the name, no assistance or band concept. Its principles
  restate § II.2 and § 6 almost exactly. Diverges on units: it carries `unit` per
  set because an interchange format must preserve what the source said, whereas
  an analytical store normalises so series compare.
- **Open exercise datasets** (free-exercise-db, wrkout/exercises.json,
  ExerciseDB) — flat catalogues with equipment/force/mechanic/muscle attributes.
  Built for exercise *pickers*, not longitudinal comparability. None attempt the
  assisted-to-unassisted collapse.
- **OpenSet** (openset.dev) — **not read.** Claims 10 execution modes and 21
  composable dimensions spanning strength, endurance and conditioning. Most
  likely place AMRAP/EMOM is modelled structurally, and the only standard seen
  spanning strength *and* cardio.
- **Sports-science literature** (NSCA-derived) — a set is a group of repetitions
  performed consecutively before stopping to rest; the rest boundary *is* the set
  boundary. Exercise means the movement. Superset = two exercises, different or
  opposing muscle groups; compound set = same muscle group. No term for the
  container.

---

## Open questions

1. **Block-level results** for AMRAP/EMOM — a score attributable to no set.
   Three workouts in the corpus repeat an exercise entry, most likely rounds
   encoded without supersets, which is evidence but not an answer. Blocked on
   seeing a BTWB export; do not design against guesses.
2. **Cluster sets** — intra-set rest. Probably already covered as separate sets,
   per the literature's rest-defined boundary, but Hevy records no rest at all so
   nothing confirms it.
3. **Per-limb load** — whether two 15kg dumbbells records 15 or 30. Resolved *by
   naming* (dumbbell and barbell preacher curls are different exercises), but
   historical recording is expected to be inconsistent. An edit-overlay problem
   (§ 7), not a modelling one.
4. **Machine identity.** A weight stack is not comparable across machines — the
   corpus carries a note reading "TechnoGym single purpose leg extension
   (settings: 2-2-max)" for exactly that reason — but the notes are too
   inconsistent to reconstruct which machine, and nothing else records it. Not a
   § 6 gap: method-dependence there concerns sensors and algorithms, and a stack
   is neither. It is an § 8 identity question, standing as a declared limitation
   on those series.
5. **Movement grouping layer** — needed for "all pull-up variants". Cheaper to
   add later than to unpick a wrong attribute schema. Not designed.
6. **Overlay anchors below the workout.** § II.2 requires overlays anchor to
   source identity, and Hevy publishes none below the workout, so the obligation
   is unsatisfiable as written. An anchor derived from content — domain
   exercise, ordinal, recorded values — survives a rebuild, which is the rule's
   stated purpose, and lapses when the set changes in Hevy, which is § II.2's own
   "overrides do not propagate" one level down. Recorded because it reads as a
   violation until that is noticed; settled when the overlay is built.
7. **Composition in the match overlay.** The overlay's vocabulary is "the same
   real-world event", which does not obviously express "is part of". Left open
   deliberately, to be settled against a concrete case.
8. **Logging as a first-class capability** — "build my own Hevy". Would make the
   platform a system of record for data no source produces, which § II.2 does not
   currently accommodate. Architectural change, not a feature. Motivated by the
   rest-recording hole.

---

## Not modelled here

- The other five entities.
- `PrescribedSet` beyond noting how it differs.
- Exercise, Movement and Variant as concrete Rust.
- Whether the effort shape is enforced structurally (sets of one kind, held by
  a sum type over kinds) or by validated construction against a kind declared on
  the exercise. The first is stronger; the choice is feature work.
