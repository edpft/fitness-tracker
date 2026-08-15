# Research: Hevy workout normalisation

Phase 0. The spec's three questions were resolved before planning and are
recorded in `docs/decisions/`; nothing here re-argues them. What follows is what
planning had to settle in order to write the design, checked against the 164
landed records rather than inferred.

Every count quoted was produced by querying `hevy_workout_landing` directly.
Sections marked superseded were overtaken by review; they are kept because what
they got wrong is worth reading. The corpus figures themselves all hold:
164 records (163 `updated`, 1 `deleted`), 1,135 exercise entries, 3,779 sets,
134 distinct templates, 361 warm-ups, 2,415 RPEs, 93 zero loads, 336 supersets
of which 2 are malformed, 1 zero-rep set.

---

## D1 — The exercise mapping lives in the Hevy adapter

**Decision**: `infrastructure/src/hevy/mapping.rs`. The vocabulary it points at
lives in `domain`.

**Rationale**: the mapping is keyed on `exercise_template_id`, which is a Hevy
identifier. A `domain` that holds Hevy's identifiers is a domain shaped by a
source, which § II.3 forbids in as many words — "no source's format shapes the
domain". Putting the key in the adapter and the target in the core is the
direction § 8 describes: sources are translated into our entities, never the
reverse.

It also makes the second source cheap in the right way. A BTWB export mapping
onto the same `Exercise` enum is a second adapter-side table; if the mapping
were in `domain`, it would be a domain that knows about two vendors.

**Alternatives considered**: the mapping as a database table, which § 9 rules
out — deterministic translation is code, and data would make it an overlay in
everything but name, editable without review and invisible in a diff. And the
mapping in `domain` keyed on an abstract "source exercise id", which buys
nothing: the abstraction has exactly one inhabitant and would still need a
per-source table somewhere to populate it.

## D2 — The mapping was derived from the corpus

**Decision**: each of the 134 templates gets an exercise, a measure and a load
reading, authored by reading what the template records and what the movement is.

1. **Measure follows the fields the template populates**, except where the model
   overrides the source's category — `Sled Push` and `Running` both do.
2. **A variant differing only in how the movement is loaded is not a different
   movement**: assisted and unassisted are one exercise, weighted and unweighted
   are one exercise.
3. **Load is `Relative` where assistance is conventionally available** and
   `Absolute` otherwise; an assisted variant negates.

**Superseded in part.** Rule 3 was originally "`Absolute` where no unloaded
version of the movement exists", and this section claimed the corpus graded it:
93 sets carry a zero load, the model of record said exactly 7 were errors, and
the load readings were right if and only if those 7 refused. It was the sharpest
assertion in the feature.

It was also wrong, and wrong in a way that a corpus cannot catch. What makes `0,
5, 10` on a good morning look like an error is that a barbell has mass — a fact
about the *implement*. The rule encoded it as a fact about the load axis, so one
distinction was carrying two jobs and the test was checking the second. Under
the rule that replaced it the seven translate, and the mapping is held to
account by asserting the axis readings directly instead.
[Decision 0004](../../docs/decisions/0004-the-load-axis-is-bidirectional-or-it-is-not.md).

**Alternatives considered**: deriving the load reading from Hevy's
`ExerciseTemplate.type`. Rejected, and this closes the open question 001 left
open. The declared type is the only published carrier of the sign convention, so
it informs the mapping when it is authored — but it cannot determine it, because
`Pull Up` is `reps_only` and `Pull Up (Assisted)` is `bodyweight_assisted_reps`
while both are one exercise here. Consulting it at translation time would also
mean a network request during a derivation that must not make one, and would make
the result depend on what the vendor's catalogue says today.

## D3 — A load never becomes a float, because it is read as bytes

**Decision**: `Kg` is a newtype over `i64` grams. The weight field is
deserialised as `&serde_json::value::RawValue`, which yields the original number
token — `77.5`, `20.4` — and that decimal string is parsed exactly into
grams.

**Rationale**: FR-014 forbids floating-point representation error, because a
load is persisted, digested and compared against rows written by earlier
versions (§ 7). The usual advice — parse to `f64`, convert to decimal — is
wrong at the first step: by the time you hold an `f64`, `20.4` is already
`20.399999999999998578…` and every later conversion is repair work.

`RawValue` removes the step. It is already how this codebase holds a payload,
so the bytes reach the translator unparsed, and reading the number's own
characters is both exact and the smallest thing that could work.

Grams rather than tenths of a kilogram: the corpus has one decimal place, the
API document promises nothing, and three spare digits cost nothing in an `i64`
— which holds ±9.2 million tonnes.

**Alternatives considered**: `serde_json`'s `arbitrary_precision` feature, which
does the same thing but as a crate-wide switch that changes how every number in
the build is parsed, including in the extraction path this feature does not
touch. `rust_decimal`, which is a dependency for one field and does not solve
the actual problem — getting the value out of JSON — any better than `RawValue`
does.

**Consequence**: `Kg` is reachable only through `TryFrom<&str>` and `Display`,
both of which speak kilograms. No caller handles the integer, which is what
keeps § 25 satisfied by a type whose inside says grams.

## D4 — The operator's zone is required configuration with no compiled default

**Decision**: `FITNESS_TRACKER_TIMEZONE`, an IANA identifier, validated at startup.
`normalise` refuses to run without it. Nothing is compiled in.

**Rationale**: § II.3 takes the zone from "declared operator configuration", and
§ 34 forbids environment assumptions. A default of `Europe/London` would be an
assumption about where the operator trains, silently correct for this account
and silently wrong for a future one — and because it would be right here, no
test would ever catch it.

The corpus supports one zone across the whole history rather than an
effect-dated series: `start_time` is a true UTC instant, and starts cluster at
18:00 UTC through BST and 19:00–20:00 UTC through GMT, a clean one-hour shift.
§ 13's effect-dating of interpretive parameters is real and is out of scope
here, which the spec says.

**Alternatives considered**: reading the host's zone. Rejected — it makes the
derivation depend on the machine that ran it, so the same raw yields different
normalised output on a laptop and a server, which breaks § 7 in a way that would
not show up until it mattered.

**The zone *database* is bundled, for the same reason and one more.** Found by
the gate rather than by argument: `nix flake check` runs the suite in a sandbox
with no `/usr/share/zoneinfo`, so `Europe/London` did not resolve and every test
that declared a zone failed. It passed locally the whole time, which is exactly
the failure mode § 34 exists to catch — "there is a system tzdata" is an
environment assumption, and a minimal container tells the same story as the
sandbox.

Building `jiff` with `tzdb-bundle-always` fixes it, and the second reason is the
better one: the zone rules are a versioned input to a deterministic translation
(§ 9), so a derivation whose wall clocks depend on which tzdata the host happens
to carry is not the re-derivable thing § 7 requires. Verified by running a
derivation with `TZDIR` pointed at nothing.

## D5 — Retraction is absorbing, and applied after the whole corpus is read

**Decision**: the use case collects retracted source record ids across every
record, then emits a workout only for ids not in that set. Not "the latest
record wins".

**Rationale**: FR-028 requires the result not to depend on read order, and
latest-wins does. It also asks a question the corpus cannot answer — whether a
source that deletes a workout and later serves it again has re-created it or is
serving a stale row — and § 10 puts that question at the canonical layer.
Absorbing declines to answer it here, which is the conservative reading and the
one that matches "a withdrawn record is not something the source is still
saying".

The corpus does not exercise it either way: the single `deleted` record names a
workout no `updated` record was ever landed for. So this is pinned
synthetically, and the synthetic pair is run in both landing orders.

## D6 — A derivation replaces the normalised layer wholesale, in one transaction

**Decision**: `normalise` deletes every row of the normalised tables for the
stream and writes the new derivation in a single transaction. No append-only
trigger, no incremental update, no upsert.

**Rationale**: § II says a derivation "is never mutated in place: after any
input changes, a derivation is identical to a full re-derivation of it". Doing
the full re-derivation every time is the cheapest way to be sure of that, and at
164 records there is nothing to optimise. It also makes SC-004 — discard and
rebuild restores it identically — the same code path as an ordinary run rather
than a second one that could drift.

The append-only triggers that guard raw are deliberately absent here. They
protect an *input*; applying them to a derivation would prevent exactly the
rebuild the constitution requires.

## D7 — A refusal carries a locus, not a message

**Decision**: `Refusal` holds the landing record, a `RefusalLocus` (the record,
or an exercise index, or an exercise-and-set index) and a `RefusalReason` sum
type. The rendered sentence is built at the edge from those parts.

**Rationale**: FR-022 asks for what, where and why, "specific enough to act on
without re-reading the payload", and FR-023 asks for it queryable. A formatted
string satisfies a reader and defeats both — SC-002 asserts the refusals are
*exactly* the named set, which is a query over reasons, not a grep over prose.

The reason distinguishes the three kinds the model of record says telling apart
is the whole point: wrong data, a declared limitation, an unmodelled case. That
distinction is the deliverable of user story 2, so it is a variant, not an
adjective in a sentence.

## D8 — What identifies a normalised workout

**Decision**: the landing record it was derived from. Not the source record id.

**Rationale**: § 10 keeps supersession at the canonical layer, so two `updated`
records for one workout produce two normalised workouts that both stand. Keying
on the source record id would make that unrepresentable and would silently
collapse the pair. The landing record id is unique, stable across rebuilds
because raw is append-only, and makes re-derivation an exact-replacement
operation.

Provenance still carries the source record id, which is what makes supersession
mechanically detectable at § 4 — that is § II.3's stated reason for requiring it.

## D9 — `NonEmpty` is ours, and so is the two-or-more variant

**Decision**: `domain::gym::nonempty` provides `NonEmpty<T>` and `AtLeastTwo<T>`.
No crate.

**Rationale**: both are a head, a tail and a constructor that rejects the short
case — forty lines, no dependency, and § 28 wants an `Arbitrary` instance
written against our own invariants anyway. `AtLeastTwo` is what makes a
single-member superset unrepresentable rather than merely rejected, which is the
difference § 24 draws.

**Alternatives considered**: `vec1` and `nunny`, either of which would do for
`NonEmpty` and neither of which offers the two-or-more case, so the interesting
half would be ours regardless.

## D10 — Measures, assigned

Nine templates are not measured in repetitions. Assigned by what they record,
with one override:

| Template | Sets | Measure |
| --- | ---: | --- |
| `Air Bike` | 32 | Duration |
| `Couch Stretch` | 28 | Duration |
| `90/90` | 28 | Duration |
| `Dead Hang` | 17 | Duration |
| `Stretching` | 16 | Duration |
| `Jump Rope` | 10 | Duration |
| `Handstand Hold` | 8 | Duration |
| `Sled Push` | 9 | Duration — **override** |
| `Walking Lunge (Dumbbell)` | 26 | Distance |
| `Farmers Walk` | 15 | Distance |
| `Running` | 19 | Distance |

Two overrides, both the same rule: our category beats the source's, and the
mapping is where that is decided. `Sled Push` is called distance-and-duration by
Hevy and holds a duration and a zero distance on all nine sets, so the zero is
never read. `Running` records a distance and a duration, and the duration was an
interval target rather than a measurement — every entry repeats one identical
pair across all its sets
([decision 0005](../../docs/decisions/0005-distance-over-time-was-prescription.md)).

The remaining 3,570 sets that translate are `ForReps`.
