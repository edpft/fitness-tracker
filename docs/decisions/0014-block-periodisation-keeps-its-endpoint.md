# 0014 — Block periodisation keeps its endpoint

**Date**: 2026-08-22
**Status**: Accepted, with one question left open
**Does not extend**: `0008-the-linear-ladder-climbs-at-a-rate.md`. That decision
removed the *linear* ladder's endpoint, and this records that it stops there.

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

## What is still open

**What a periodised block does when a lift is missed.** Nothing was decided, and
nothing should be invented.

The literature gives phase structure, deloads and tapers; it does not hand over a
per-miss protocol. Loads in accumulation are deliberately submaximal — volume is
the stimulus, not proximity to failure — so the classical assumption is that
misses do not happen, and the operator's instinct matches it. The modern patch is
autoregulation: an RPE or RIR cap, or a velocity cut-off, turning the planned
percentage into a ceiling adjusted to the day.

**The proposal on the table is diagnosis rather than adjustment.** A miss in
accumulation is not evidence that the week was too heavy; it is evidence that the
*entry test* was too high. So the block plans what it plans, and repeated misses
are reported as "the entry test looks too high" rather than silently corrected.
That needs no invented number, and the block already ends in a test that measures
the truth.

Two things would have to be settled to accept it: whether diagnosis is the right
response at all, or whether the top set should be autoregulated — which changes a
prescription from an instruction into an advisory — and how many misses count as
repeated. The second is a number, so it is the operator's.

Until it is settled, a periodised block prescribes its plan and records what
happened, which is what it does today.

## Consequences

- `Block::ENDPOINT` stays, and no future decision about linear's rate reaches it.
- Wiring block is not a translation of the linear path: the two share a calendar,
  a weekday map, fills and an anchor, and diverge at what a week prescribes.
- `prescription::Programme` is `linear::Programme` re-exported at the crate root,
  which cannot survive a second template. The name has to become general or move.
