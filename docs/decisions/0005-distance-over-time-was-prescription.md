# 0005 — Distance-over-time was prescription

**Date**: 2026-08-14
**Status**: Accepted
**Supersedes**: [0002](0002-distance-and-distance-over-time-are-different-measures.md).
**Raised by**: review of `specs/002-hevy-workout-normalisation`.

## What was decided

There are three measures again: repetitions, elapsed time, and ground covered. A
run is ground covered, like a carry. The duration recorded alongside it is not
part of the observation.

## Why

Decision 0002 split ground covered from ground covered in a time, on the argument
that a carry is time under load and a run is pace. The split was defended with
evidence that looked strong: every one of the corpus's `Running` sets carries a
duration and not one of its 41 carry sets does, so no exercise was left
ambiguous.

Looking at the values rather than at whether they were present says something
else:

```
2025-03-10  Running  400m/150s, 400m/150s, 400m/150s
2025-03-31  Running  400m/120s, 400m/120s, 400m/120s
2025-04-07  Running  200m/60s ×5
2025-04-14  Running  200m/60s ×5
```

Every set within an entry is identical, and identical across the entry is the
signature of a target rather than a measurement. Nobody runs three 400s in
exactly 150 seconds each. What is written down is "3 × 400 m on 150", which is
an interval prescription that Hevy had no prescribed side to put anywhere else.

The clean presence/absence split that made 0002 look well-evidenced turns out to
be the same fact seen from the wrong angle: the duration is present exactly where
a target was set, which is a fact about how the operator uses the app rather than
about what a run is.

## Consequences

- `TimedDistance` and `TimedDistanceExercise` are gone; `Running` joins
  `FarmersWalk` and `WalkingLunge` in the distance vocabulary. 19 sets keep their
  distance.
- The duration is not carried. It is a target, § 11 stores prescription
  separately, and this feature has no prescribed side — so recording it here
  would be intent leaking into an observation, which is precisely what the model
  document warns happens when the split is missing.
- 0002's actual argument — that an always-absent optional field merges "not
  captured" with "does not apply" — is untouched and was never the problem. It
  simply no longer has two measures to arbitrate between.
- If a source ever records what a run actually took, that is a fourth measure
  and this decision is revisited with evidence rather than reversed on principle.
