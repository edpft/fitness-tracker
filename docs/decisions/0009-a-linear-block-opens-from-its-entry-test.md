# 0009 — A linear block opens from its entry test

**Date**: 2026-08-19
**Status**: Accepted
**Closes**: D8 in `specs/003-prescribed-workout-generation/research.md`, and with
it T080 — the last open task of that feature.
**Follows**: `0008-the-linear-ladder-climbs-at-a-rate.md`, which removed the
ladder's endpoint. This removes its opening.
**Scope**: the `linear` template only.

## What was decided

**A linear block is preceded by a test, and where it opens follows from what that
test did.**

```text
the test failed a load     the block opens at that load, and climbs in to it
the test failed nothing    the block opens one climb above what it completed
```

"Climbs in" means the drop-and-re-climb protocol, unchanged in mechanism: the
**second** reset's −5% from the failed load, then +2.5kg a week back up to it, at
which point the ladder takes over at its first rung — which is that same failed
load.

**The second reset's protocol rather than the first**, for two reasons. The first
reset is the steeper pair — a 10% drop re-climbed at 5kg a week — because it is
recovering ground a stall has just cost; an entry has lost nothing and is
approaching a load the lifter has never held. And the second reset's rate *is*
`ladder_climb_per_week`, which `docs/primary-lift-progression.md` already calls
"baseline rate off a lower start" — so the climbing-in weeks and the ladder weeks
advance by the same increment and the block is one continuous climb with no seam
where the approach becomes the plan.

**The entry climb spends no stall.** A block that opens this way still has both
resets available to its first real failure. `ClimbBack::Entry` carries that, and
it is a separate variant from `ClimbBack::Reset` precisely so nothing can count
it.

Consequences in the model:

- `Anchor` carries `failed: Option<Kg>` beside its completed load. The anchor is
  the test's whole outcome rather than its best set.
- `GenerationParameters::ladder_start` is gone. Nothing about where the block
  opens is authored.
- `Programme::new` refuses a block whose entry test is not before it.

## Why

**Because it removes the last authored number, and the operator proposed it.**
Offered the choice between authoring an opening percentage and deriving one, they
described the derivation:

> We assume that a linear block will also start with a pre-block test week and we
> start by using the drop reclimb protocol from the failed lift, or test result +
> 2.5kg if no lift was failed, however, in this case it shouldn't count towards
> the reclimb protocol count, so the next failure will get 2 drop and reclimbs.

`ladder_start` had been `TODO` since the feature began and was the whole of what
still blocked SC-001. It is not replaced by another parameter: it stops existing.

**The two branches say different things and the model now holds both.** A test
that failed something located the ceiling; a test that failed nothing only
established a floor. Reading the second as "the maximum is what was completed"
would be an assertion the evidence does not support, which is why
`anchor_failed_grams` is nullable and why the two branches open differently.

**It is the same mechanism, not a new one.** Mid-block, a stall drops from the
failed load and climbs back to it. At the entry, the block drops from the failed
load and climbs back to it. Writing that as one code path is what makes the entry
free of new concepts — and what makes "does it count as a stall?" the only
question that needed answering, which the operator answered.

**The entry test must precede the block, and that is now enforced.** The test
session is in the performed record. A block containing its own entry test would
read that failure twice — once as the opening it derived from, once as a missed
gating set inside the block — and prescribe from a progression that had counted
one event as two. `Programme::new` refuses it. The fixture used by the
integration suite had exactly this shape and had to be moved.

## What it costs

**The plan it produces is more ambitious than the old one, and than what the
operator has been running.** From the corpus's own entry test — 90 completed, 95
failed on 2026-07-03 — an 8-week block issues:

```text
week    1     2     3     4     5     6     7     8
load    90    92.5  95    97.5  100   102.5 105   test
        └ climbing in ┘   └────── the ladder ──────┘
```

One increment a week from 90 to 105, against a tested 90kg. The old span model
finished at 94.5, and the operator's own hand-run block through July and August
went 82.5, 85, 87.5 with 92.5 planned for 28 August.

The ladder itself runs to 110 at its seventh rung, but a block only reaches that
if nothing is spent climbing in and nothing stalls — which is the plan being an
intention rather than a prediction, and is the same thing `programme show`'s
table has always displayed.

That is not hidden and it is not obviously wrong: the block opens above the
anchor because the anchor is a *completed* single and the test proved 95 was
reachable-but-missed, and the whole design says the reset protocol — not the
plan — is what discovers the ceiling.

**What would soften it further, if the operator wants that**: lower
`ladder_climb_per_week`, or open the ladder at the *dropped* load rather than the
failed one, which would be a different decision from this one and should be
recorded as such.

## What is still open

Nothing in this feature. `crates/infrastructure/tests/fixtures/programme.toml`
carries no `TODO`, `fitness programme author` accepts it, and `fitness prescribe`
issues from it. The refusal mechanism for an unsettled value is retained and
tested by injecting a `TODO`, because the next authored value will need it.

## Consequences

- `Ladder` holds a load rather than a percentage, and `heavy_top_set` no longer
  takes an anchor: the anchor is read once, at construction.
- `Ladder::implied_percentage` is the only place a percentage of the anchor
  appears, and it is reporting. Under this decision it routinely reads above
  100%, which is the plan being visible rather than a fault.
- The corpus no longer contains an in-block miss, because its only failure is the
  entry test that now precedes the block. US3-5's integration test is replaced by
  one asserting the entry climb; the miss-holds-the-ladder arithmetic stays in
  `crates/domain/tests/progression.rs`, where it always was.
- Migration `0010` adds `anchor_failed_grams` to `programme` and
  `prescribed_workout` and drops `ladder_start_bp`.

## Amended 2026-08-20

Two changes. The heading still holds — a block opens from its entry test — but
both halves of *how* were wrong.

**The drop is the opening, not a climb-in to it.** This decision had the ladder
open at the failed load and reach it by drop-and-re-climb at the second reset's
−5%, so `ClimbBack::Entry` existed to keep that climb from spending a stall.
Week one was therefore heavier than the anchor, which `primary-lift-progression.md`
described as "ambitious" and defended. The operator overturned it: the ladder
opens at the failed load dropped by an authored `entry_drop` of −10% and climbs
back *through* it. `ClimbBack` is gone, because every re-climb inside a block is
now a stall and there is nothing left to tell apart.

**And the derivation can be overridden.** The block starting 3 August is why.
Its entry test is dated 3 July, with a hand-run block and a fortnight's holiday
in between, so nothing derived from that test is evidence about where this block
should open. The tell is that two different rules reproduce the operator's
stated 85 exactly — −10% off the failed 95 is 85.5, and −5% off the completed 90
is also 85.5, and the 2.5kg grid takes both to 85. Two rules agreeing on one
observation means the observation evidences neither.

So `programme.opening` states the load and the anchor's failed load feeds
nothing. This is not a fallback for an unauthored parameter, which is what the
original decision was written to eliminate: it is the answer where the
derivation has no standing. The operator's words were that we "always need the
escape hatch of a declared entry point" — always available, never conditionally
required, so nothing asks how old a test is.

The derivation remains the default and is what the block starting 21 September
will use, since that one follows its own test directly. Nothing in the 3 August
block exercises it, so `crates/domain/tests/ladder.rs` pins it at an anchor
where the two candidate rules disagree.

## Amended 2026-08-22

**The recency judgement became a rule.** The amendment above argued that the 3
July test had no standing over a block starting 3 August — a month and a hand-run
block in between — and left "how old is too old" to the operator, with a declared
opening as the escape hatch.

`0013-a-test-belongs-to-one-programme-or-to-none.md` states the threshold: a
preceding test is usable as input when it is **the same exercise** and falls in
**the week before the programme or the week before that**. The 3 July test fails
that by four weeks, so the declared opening stops being an escape hatch and
becomes the only correct answer — same outcome, no discretion.

Two consequences for this decision. `EntryTestIsNotBeforeTheBlock` is now the
weaker half of the check: the test must precede the programme *and* be recent.
And the exercise is part of what makes a test inheritable at all, which this
decision never said — there is no relationship between a front squat maximum and
an RDL one, so a test in the wrong lift is not a stale input but no input.
