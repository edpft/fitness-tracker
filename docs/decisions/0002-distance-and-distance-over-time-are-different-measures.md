# 0002 — Distance and distance-over-time are different measures

**Date**: 2026-08-14
**Status**: Accepted
**Revises**: `docs/gym-workout-domain-model.md` — the declared `Distance` type, and
open question 2.
**Raised by**: `specs/002-hevy-workout-normalisation`, question Q3.

## What was decided

The exercise vocabulary partitions by four measures, not three: repetitions,
elapsed time, ground covered, and ground covered in a time. A carry and a run
are not one measure recorded to different depths.

The model of record declared `struct Distance { metres: Metres, duration:
Option<Duration> }` and named the alternative as an open question. The
alternative wins.

## Why

**The corpus splits totally.** All 19 `Running` sets carry a duration. Not one
of the 41 carry sets — `Farmers Walk` (15), `Walking Lunge (Dumbbell)` (26) —
carries one. There is no exercise the evidence leaves undecided, which was the
stated cost of splitting: "a decision per exercise that the corpus may not
settle". It settles every one of them.

**An always-absent option is not partial data.** § 37 covers a value that
applies and was not captured. A farmer's walk has no duration to capture; the
field would be absent for every such set ever recorded. Under the optional
duration, `None` would mean "not captured" for a run and "does not apply" for a
carry, with nothing in the type telling them apart.

That is the same defect the model already rejected once. "There is no
'unrecorded' load" removed a variant precisely because it merged data that is
wrong, a value that does not apply, and a value that applies and was never
captured. Keeping the optional duration would have reintroduced the merge one
field over.

**The split is what makes a wrong comparison fail to compile.** A carry is time
under load; a run is pace. With one measure, a series averaging pace across both
is expressible and merely wrong. With two, it does not typecheck — the same
reason RIR is eight named positions rather than a number.

## Consequences

- `Distance` loses its optional duration and becomes ground covered alone. A
  fourth measure carries ground covered together with the time it took, and
  neither field is optional in it.
- The vocabulary gains a fourth partition. An exercise's measure is still fixed
  by which vocabulary it belongs to, so a set and its exercise still cannot
  disagree.
- No count changes. The same 3,779 sets translate or refuse as before.
- `Sled Push` is unaffected: it records thirty seconds and a zero distance, and
  remains a duration exercise regardless of the category the source declares.
- If a future source records a carry's duration, that carry's exercise moves
  vocabularies. That is a mapping change, which is code and version-controlled
  (§ 9), and it is a visible one — which is the point.
