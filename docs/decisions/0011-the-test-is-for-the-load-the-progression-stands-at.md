# 0011 — The test is for the load the progression stands at

**Date**: 2026-08-21
**Status**: Accepted
**Follows**: `0008-the-linear-ladder-climbs-at-a-rate.md` and
`0009-a-linear-block-opens-from-its-entry-test.md`, which between them made the
ladder a rate from a stated opening. This says what the week after the last rung
is an attempt at.
**Scope**: the `linear` template's test week.

## What was decided

**The test week is an attempt at a load, and that load is where the progression
stands.** Three cases, and they are one rule:

```text
every rung made      the position runs past the ladder    last rung + one climb
a rung missed once   a miss holds, so the position is it   the rung's own load
a stall, re-climbing the drop is where you are, not what   the load being
                     you are testing                       climbed back to
```

**The ramp is built off that target**, not off the anchor. A test week has no
rung, so the warm-up percentages had nowhere to point and were taken of the
anchor; they are now taken of the target.

**The top set stays autoregulated.** The target is what the ramp is built toward
and what the report names. Going past it is the outcome the block exists to
produce, so nothing caps it.

## Why

**Because ramping off the anchor had the operator working up to a number they
passed three weeks earlier.** The anchor is fixed for the block by design — only
the exit test replaces it, and that replacement anchors the *next* block — so it
is the one load in the model guaranteed to be stale by the time the test arrives.
A block anchored at 90 that has climbed to 95 should warm up toward 95.

**The three cases were stated by the operator on 2026-08-21**, over a seven-week
block opening at 85 and climbing 2.5 a week: rungs 85, 87.5, 90, 92.5, 95, 97.5,
then the test.

The third is the one worth stating. A re-climb sits *below* what was failed — two
misses at 95 drop to 85 — and it would be easy to read the position as the drop.
It is not: the test asks whether the failed load goes up now, not whether the
drop does. The load being climbed back to is already carried by
`Progress::ReClimbing { toward, .. }`, so the rule reads the field that already
exists rather than reconstructing anything.

**And the first case needs a load the ladder does not have.** A block whose every
rung went up has nothing left to prescribe and a test to do.
`Ladder::beyond` is what the next climbing week would have asked for, had the
block had one — derived from the authored rate rather than chosen separately, so
no new number enters the model.

## What it costs

**The target moves as the record does, so it is not something the plan can
promise.** Every rung that goes up raises it. That is why `programme show` prints
it as a line below the table — "the test is for 95 as the record stands" —
rather than as a cell inside it. The test row's load column reads `—`: the plan
does not know, and a number printed in the plan's own table would claim it does.

**The first case tests a load the plan never prescribed.** One climb past the
last rung is above every rung the block issued. That follows from the block
having gone perfectly, and the reset protocols are what handle it going
otherwise — but it is worth seeing plainly, because it means a flawless block
ends by attempting something new rather than by confirming something done.

## Consequences

- `Progress::test_target(ladder, steps)` is the whole rule, and it is on
  `Progress` rather than on `Ladder`: the ladder is the plan, and where the
  progression stands is a fact about the record.
- `Ladder::beyond(steps)` gives the rung past the last, quantised on the same
  grid as every other rung.
- `application::prescribe` derives the test week's ramp from the target, and
  reports the slot underivable if the ladder cannot be built — the same refusal
  the climbing weeks already make.
- `programme show` gains one line; the test row of the ladder table loses its
  second `test` cell.

## What is still open

**A test that belongs to neither block.** A test belongs to a block, and this
decision says what its target is when it is the block's *exit* test. The session
on Friday 18 September is exactly that, so nothing is blocked today.

But the same session was floated as the *start* of the autumn block, and a block
may not contain its own entry test: `Programme::new` refuses it, because the
failure would be read twice — once as the opening it derived from, once as a
missed gating set inside the block (decision 0009). There is currently no way to
prescribe a test that belongs to neither block on either side of it.

Worth settling before the autumn block is authored, not now. "The exit test of
the summer block" is a true description of 18 September, and a true description
is enough to prescribe from.
