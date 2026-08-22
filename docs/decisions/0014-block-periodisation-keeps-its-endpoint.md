# 0014 — Block periodisation keeps its endpoint

**Date**: 2026-08-22
**Status**: Accepted
**Does not extend**: `0008-the-linear-ladder-climbs-at-a-rate.md`. That decision
removed the *linear* ladder's endpoint, and this records that it stops there.

## Amended by 0016

**Week 1 is an entry test only where the block says so.** This decision
described every block as carrying an entry test as its first week, with
`total_weeks` one longer than the duration. It is now optional: a block either
measures its own entry in a week in front of the phases, or opens from a maximum
measured before it — a standalone test, or the previous block's exit test — in
which case its weeks are its phase weeks and nothing else.

`total_weeks` and `WeekPlan::EntryTest` are gone with it. `Block` plans phase
weeks and knows nothing about a measurement week; the programme that owns one
puts it in front. `entry_reps` moved to the entry test itself, which is where the
repetition count belongs — by the time the phases read the number it is an
`Anchor` and already a one-rep maximum.

Everything else here stands. The endpoint is still 105% of the entry one-rep
maximum, the split is still one rule, and the block still ends on its exit test.

## What was decided

**A periodised block plans toward a specific endpoint. A linear programme does
not.** `Block::ENDPOINT` stays at 105% of the entry maximum, whatever the
duration.

The two are not versions of one model that fell out of step. In the operator's
words on 2026-08-22:

> linear periodisation doesn't build towards a specific endpoint, it's inherently
> autoregulated, you take a starting point and you just keep adding weight until
> you can't any more, but block periodisation does build towards a specific
> endpoint.

So the divergence is the point. Linear discovers a ceiling by running into it,
which is why it grew a stall protocol and a re-climb. Block plans a peak from a
measured start, which is why it grew phases and an exit test. Each has the
machinery its own model needs.

## Why this needed recording at all

**Because from the code it looks like an oversight.** `block.rs` has not been
touched since `294350b`, the 003 merge. `ladder.rs` has been revised three times
since — 0008 removed its endpoint, 0009 and its amendment changed where it opens,
and 005 added `beyond`. A reader finding a fixed `ENDPOINT` in one and a rate in
the other would reasonably conclude the first had been missed.

It had not been. But three things in `block.rs` genuinely *are* unrevised, and
have to be checked as it is wired rather than assumed to have survived:

- it contains its own entry test — `total_weeks() = duration_weeks + 1`,
  documented as "entry test included" — which decision 0013 now forbids;
- it predates 0010, so it has never met a calendar whose weeks can be skipped
  by session;
- it predates 0011, so nothing in it knows what its exit test is an attempt at
  beyond the endpoint it planned.

## What a failure does: it exits the block

**A missed lift ends the periodised block.** Not a retaken week, not a drop and
re-climb, and not an autoregulated ceiling. The block stops, and what it has
produced is the information that it stopped.

**Because a block is a declared number of weeks aimed at a declared endpoint.**
That is the whole difference from linear, and it is the operator's argument on
2026-08-22: retaking a failed week, or dropping back and re-climbing, pushes the
endpoint out and changes the shape of the plan — a knock-on that a linear
programme does not have, because a linear programme has no declared target and no
exit test to arrive at. Linear can absorb a stall by spending weeks it never
promised. A block cannot spend a week without becoming a different block.

**And a failure inside a block is evidence about its inputs, not about its
week.** Either the entry test was wrong or the progression schema is too steep.
Both are settled before the block begins, so neither is reachable from inside it,
and there is nothing to autoregulate toward: the plan's percentages come from the
entry maximum, so a top set that fails says the maximum was overstated.

Three consequences follow, and none of them needs a number:

- **The exit test does not happen.** A block that failed did not reach
  realisation, so there is no peak to measure. The failure is the outcome.
- **A failed block yields no entry test for what comes next.** A missed working
  set is not a test, so under
  `0013-a-test-belongs-to-one-programme-or-to-none.md` a block following a failed
  block needs a standalone test, and a linear programme following one declares
  its opening.
- **Nothing further is prescribed from that programme.** Its remaining scheduled
  days belong to no programme, which 0012 already makes a real state rather than
  an error.

**One detail left to settle in the building, not here**: what counts as the
failure. The reading that matches linear is the gating session's top set, since
that is already the only performed set the progression reads — a missed accessory
is not evidence about a front squat maximum. If the operator means something
wider, it is a change to this decision rather than an implementation choice.

## Consequences

- `Block::ENDPOINT` stays, and no future decision about linear's rate reaches it.
- Wiring block is not a translation of the linear path: the two share a calendar,
  a weekday map, fills and an anchor, and diverge at what a week prescribes.
- `prescription::Programme` is `linear::Programme` re-exported at the crate root,
  which cannot survive a second template. The name has to become general or move.
