# 0006 — Prescription reads the normalised layer

**Date**: 2026-08-19
**Status**: Accepted
**Raised by**: `specs/003-prescribed-workout-generation`, research D3.

## What was decided

Generation reads the **normalised** layer, not the canonical one. § II.4 says
the canonical layer "is the clean layer the analytical layer reads", and
prescription is not the analytical layer — but it is a consumer of observations,
and the layer it should read is the one that exists.

Same-source supersession is resolved in the adapter, in a `WHERE` clause on
every query that reads performances. Cross-source matching is not resolved at
all, because there is one source.

## Why

**The canonical layer has not been built, and building it for this would build
the wrong thing.** § II.4's job is one entry per real-world event whatever
number of sources recorded it. With one source there is nothing to match
against, so a canonical layer today would be a copy of the normalised one with a
second identity scheme — and the first thing that needs it for real, a second
source, would then find a layer shaped by having had none.

**But supersession could not be deferred with it.** The spec defers the
canonical layer on the grounds that one source needs no reconciliation. That is
sound about *matching* and wrong about *supersession*: § 10 says two landing
records sharing a source record id are the same source contradicting itself, and
the later supersedes. A projection reading both would prescribe from a
performance the source has withdrawn — silently wrong rather than visibly
broken, which is the failure mode this repository spends most of its effort
avoiding.

So the currency clause is in the queries:

```sql
AND NOT EXISTS (
      SELECT 1
      FROM gym_workout AS superseding
      JOIN hevy_workout_landing AS later ON later.id = superseding.landing_record_id
      JOIN hevy_workout_landing AS this  ON this.id  = w.landing_record_id
      WHERE superseding.source_record_id = w.source_record_id
        AND later.serve_ordinal > this.serve_ordinal
)
```

It appears in full in every query rather than in a shared constant, because
`sqlx::query!` verifies SQL against the schema at compile time and will not
accept an interpolated string. Offline verification is worth more than one fewer
copy.

**No such pair exists in the corpus.** 165 records, 165 distinct source ids, no
re-serve. Which is exactly why this was cheap to do now and would have been
expensive to find later — the query is right before there is any data to prove
it wrong with.

## What stays unresolved

- **Fragmentation.** One training session spread across four landing records
  stays four workouts. Harmless for "the most recent performance of this
  exercise", and not harmless for a session count, a frequency or a streak — §
  10's counting rule. The first figure that needs it right is the trigger to
  build the canonical layer properly, and that is a better trigger than this
  feature was.
- **Cross-source matching**, which is what § II.4 is actually for and what a
  second source will need.
- **Whether prescription should read the canonical layer once it exists.**
  Probably yes, and the queries that would change are two.

## Consequences

- `SqliteExerciseHistory` and `SqlitePerformedWorkoutReader` both carry the
  clause, and both say so in their module notes. A third reader must too, and
  the tests do not catch its absence — nothing in the corpus is superseded.
- The deviation is recorded here rather than argued in the code, so "why does
  prescription not read the canonical layer" has one answer in one place.
- § II.4 is not amended. It describes a layer that will exist and says what will
  read it; this says what reads the layer that does exist today.
