# 0007 — A zero-rep set is a failed attempt

**Date**: 2026-08-19
**Status**: Accepted
**Supersedes**: the behaviour `specs/002-hevy-workout-normalisation` shipped.
**Raised by**: `specs/003-prescribed-workout-generation`, user story 2.

## What was decided

A set recorded with zero repetitions is a **failed attempt**: an outcome the
normalised layer holds, carrying the load that was on the bar and no count at
all. It was a refusal — `RefusalReason::ZeroReps`, kind `unmodelled` — and that
reason is now deleted.

`RepCount` stays non-zero. The zero is a sentinel read in translation into
`Performed::Failed`, so no arithmetic can take a quantity from a failure: there
is no measure to take.

## Why

**002 was right that zero is not a count and wrong about where the case goes.**
Its own migration says so in a `CHECK`: "a rep count of zero is an attempt, not
a set". An attempt needs a prescribed side to have meaning — something it was an
attempt *at* — and 002 had none, so the honest move at the time was to refuse it
and say which of the three refusal kinds it was. It was filed as unmodelled,
which is the kind that means "refine the model", and this is that refinement.

**What made it load-bearing is the negative gate.**
`docs/primary-lift-progression.md` detects a stall from a miss: a miss holds the
ladder, and a second miss at the same load suspends it. A failure the normalised
layer will not represent is therefore a stall the programme cannot see, and the
programme is the deliverable.

## The discriminator is the repetition count, and this is the trap

Hevy has a `failure` **set type**, and it is not the discriminator. It means
"taken to failure" — a note about effort, which the translator already reads
correctly as zero repetitions in reserve — and it sits on **77 sets** in the
corpus. Exactly one of those 77 carries zero repetitions: the 95kg front squat
of 2026-07-03.

Keying on the type would file 76 completed working sets as failed attempts and
take their volume out of every total. The test asserts both numbers, counted
from different places — 77 from the raw payloads, 1 from the derived layer —
because a test that only counted the one it wanted would pass against the wrong
implementation.

**An absent count still refuses.** A source serving no value at all on an
exercise counted in repetitions has told us nothing, which is a different thing
from telling us the lift was missed.

## Consequences

- Sets in the normalised layer rise from 3,778 to 3,779 and refusals fall from
  three to two. Intensities rise from 2,414 to 2,415: the failure carries its
  recorded RIR, because a failure carries no *measure* and that is a different
  absence from carrying no effort.
- **No reason maps to `RefusalKind::Unmodelled` any more.** The kind stays in
  the vocabulary: the next thing the domain cannot hold needs it, and an
  operator has to be able to tell "refine the model" from "fix the record"
  without opening a payload.
- Migration `0008` deletes the stored `zero-reps` row. This build cannot parse
  that key, so the refusal report would fail rather than look stale. Deleting
  from a derivation is not deleting a fact — the raw payload is untouched and
  the next `normalise` rewrites every row there regardless.
- **A failure and an absence are now distinguishable, and the model needed
  telling.** Both answer `None` when asked for a quantity. What separates them
  is that one is a set and the other is not, which is why `Performed<M>` is a
  sum type rather than an `Option<M>`.
- The round trip found the sharpest consequence: a failed attempt carries no
  *intended* count either, because nothing records what was being attempted.
  That is a `ProjectionGap::IntendedMeasureUnknown` rather than a guess, and it
  says the performed model still cannot fully describe a missed set.
