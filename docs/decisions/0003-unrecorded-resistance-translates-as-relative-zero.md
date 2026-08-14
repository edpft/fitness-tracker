# 0003 — The air bike and the sled translate as `Relative(0)`

**Date**: 2026-08-14
**Status**: Accepted, as a declared limitation
**Resolves**: `docs/gym-workout-domain-model.md`, open question 5.
**Raised by**: `specs/002-hevy-workout-normalisation`, question Q2.

## What was decided

Two exercises carry resistance that is neither bodyweight nor recorded: the air
bike's fan (`Air Bike`, 32 sets) and the sled's load (`Sled Push`, 9 sets). Their
41 sets translate, with `Relative(0)` as the load, and the fact that this load is
not a measurement of what was moved is declared here rather than expressed by
the value.

## Why

The alternatives each cost more than the limitation does.

**Distinguishing load-applicable exercises in the vocabulary** is the honest
option, and it is what the model's own reasoning points at: `Relative(0)` means
"plain bodyweight" for every other exercise, so for these two it asserts
something false. But the distinction is paid for by two exercises out of 134, it
changes `Set`'s shape for all of them, and it puts a second reason a load can be
absent into a type whose whole argument is that every set has one.

**Refusing them** keeps the model unchanged and the gap visible, at the cost of
41 sets that are otherwise well recorded. An air bike set holds a duration, and
that duration is a good observation; discarding it because a second field was
never captured loses a real fact to protect a field nobody has asked a question
of yet.

## What is being accepted

`Relative(0)` on these two exercises means "the resistance was not recorded",
where everywhere else it means "no load beyond bodyweight". Deterministic
translation cannot tell the two apart from the data, which is exactly why the
model rejected an `Unrecorded` load variant — so this is that ambiguity,
readmitted for two exercises and confined to them by the mapping.

It is bounded, and it is bounded in code: the mapping is the only place that
decides an exercise's load interpretation, so the exercises this applies to are
enumerable by reading it. Nothing analytical consumes load for either exercise
today.

## When to revisit

When something asks a load question of a conditioning exercise — sled volume,
air bike work — or when a source starts recording either resistance. The answer
then is the vocabulary distinction rejected above, and the cost of arriving at it
late is a mapping change plus a re-derivation, both of which are cheap by
construction (§ 7).
