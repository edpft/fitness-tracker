# Primary Lift Progression

Governs the primary strength slot only. Non-primary slots progress by double
progression against observed history and are out of scope here.

That split is deliberate and is the reason this document exists. The rest of the
session works well on double progression; only the primary lower-body lift needs
something else, because only the primary has a 1RM the programme is trying to
move.

## Position

The prescription is planned in advance and executed as written. There is no
scoring, no rep range, and no effort report feeding any decision.

RIR is captured on the primary's sets but is not an input to any derivation. It
is an observation, retained in case a retrospective check is ever wanted.

The rationale is resolution. A gate reading a signal finer than the operator can
discriminate does not autoregulate — it introduces a decision point resolved by
mood. On lower-body lifts the discriminable states are coarse, so no gate is
better than a gate that pretends otherwise.

**A plan and a mechanism for handling failure are two different things.** This
document holds both, and conflating them caused a wrong turn once already. The
plan is § The plan: a ladder generated from a duration and a starting 1RM. The
failure mechanism is § Stall detection and § Reset protocol, which take over
when the plan turns out to have been too ambitious. Neither is derived from the
other, and the plan is not designed around the possibility of failing.

## What the generator is for

Given **a number of weeks** and **an entry test**, generate a programme which,
if performed, leaves the tested 1RM at the end higher than it was at the start.

That is the whole requirement. Calendar goals, target numbers for the year, and
what to do about a stall are all outside it.

No template can guarantee the outcome. What one supplies is a structured
overload at a defined rate, and a test that says whether it worked.

## The anchor

The anchor is the **entry test's outcome**, and it is **fixed for the duration
of the block**. It carries two things: the heaviest single completed, which is
the starting 1RM, and the load failed above it if the test found one. Both are
evidence and the block reads both — see
`decisions/0009-a-linear-block-opens-from-its-entry-test.md`.

**The test precedes the block it anchors, and a block may not contain its own
entry test.** The test session is in the performed record, so a block holding it
would read that failure twice: once as the opening it derived from, once as a
missed gating set inside itself. Authoring refuses it.

It carries its provenance, because the four ways of arriving at it are not
equally good: a **test** is measured, an **e1RM** is derived, an **asserted**
value is neither. Once a series of blocks is running the exit test of one block
is the entry anchor of the next, so there is one source; the others are
bootstraps, used for the first block or after a gap.

**The anchor does not climb.** An earlier version of this document had *the
anchor* advancing +2.5kg per week, which is a different thing from the ladder
climbing at that rate and is still wrong: every load is a share of the anchor, so
an anchor that moves re-bases the warm-ups and the back-offs with it and no two
weeks are comparable. What climbs is the ladder's position, and the anchor stays
where the test put it until another test replaces it.

**Only a test replaces it**, and a test ends a block. So within a block the
anchor is a constant, and nothing performed moves it — which is what makes the
whole prescription computable in advance from two inputs.

**An e1RM from a submaximal set is not a measurement.** A set left with
repetitions in reserve says nothing about a maximum, whatever a formula returns
for it. Only a set taken to failure, or a genuine single, supports an estimate.
This is worth stating because the arithmetic is available on every set in the
record and is meaningless on almost all of them.

## The plan

**A linear block: intensity ascends across the duration, and the block ends in a
test.** The standard template, and the one that takes exactly the two inputs
above.

Given a duration of `W` weeks and an anchor `A`:

- The final week is the test, so there are `W - 1` climbing weeks.
- The ladder opens at the load the entry test failed, or one `climb_per_week`
  above what it completed if it failed nothing, and adds `climb_per_week` for
  each week after the first.
- A block whose test failed something **climbs in** to that load first, by the
  drop-and-re-climb protocol below at the **second** reset's −5% and +2.5kg —
  the gentler pair, because an entry has lost no ground and because that rate is
  the ladder's own, so the whole block advances by one increment a week. Those
  weeks are not ladder positions and they cost no stall.
- The light session's top set is a percentage of that week's heavy top set.
- Warm-ups and back-off sets are percentages of their own session's top set —
  never of the anchor. See `prescribed-workout-domain-model.md`.

**The rate is authored and there is no endpoint.** Settled by the operator on
2026-08-19: a linear block picks a starting point and attempts to add a fixed
increment every week, and what regulates the climb is the reset protocol below
rather than a stated top. See
`docs/decisions/0008-the-linear-ladder-climbs-at-a-rate.md`, which records the
argument this paragraph used to make against exactly that and why it does not
hold — in short, it assumed the plan is what has to stop the climb, and something
else already does.

**Where the rate comes from.** 2.5kg is the smallest plate, so it is the
slowest honest climb and it lands every rung on the grid at every anchor. It is
also the rate the reset protocol below already names: the second reset re-climbs
at +2.5kg a week, which that section calls "baseline rate off a lower start".
The two are one number and it was stated in prose before it was a parameter.

**Where the opening comes from — the test, and nothing else.** A test that failed
a load located the ceiling, and the block's job is to reach it and go past; a
test that failed nothing established only a floor, and the block starts by
beating it. Neither branch asks anyone for a percentage, which is what closed the
last unauthored value in the model. **The plan this produces is ambitious**: a
block opening on a failed load starts above its own anchor, and it is the reset
protocol rather than the plan that finds the real ceiling.

**What duration does here, and what it does not.** It says how long the climb
runs. An interrupted eight weeks is the same plan as a twelve stopped earlier,
which is the honest description of a block broken by a holiday and is why
`linear` is the template for a short or interrupted window. Where duration
genuinely shapes the plan is `block`, which sets its rung count and phase split
from it.

Personal history still bounds how far a block can reasonably get, and the
boundary that matters is **regain versus new ground**: ground already covered
comes back fast, and ground never covered does not. A block that spends its weeks
below a previously demonstrated max is asking for regain and is the safer first
block.

**Repetitions are constant per session role within a block.** Currently one on
the heavy session and three on the light one. The textbook linear block descends
the reps as the intensity climbs — fives, then threes, then singles — and this
one does not, because the record does not: the rep counts have been fixed per
role since the July test while the load climbed. Descending reps is a legitimate
variant and is deferred rather than rejected; it changes the ladder from a
series of loads into a series of `(load, reps)` pairs and nothing else.

## Stall detection

This and the section below are the failure mechanism. They are not part of
making the plan.

- A **miss** holds the ladder and re-issues the same loads the following week.
- A **second miss at the same load** is a stall and triggers a reset.

Because a miss re-issues the week, loads necessarily repeat, so "same load
twice" is always reachable. No further sequencing rule is required.

## Reset protocol

| | Drop | Re-climb | Round trip |
|---|---|---|---|
| **Reset 1** | −10% | +5kg weekly | 4 weeks |
| **Reset 2** | −5% | +2.5kg weekly | 4 weeks |

The drop and the increment are chosen as a pair so both land on the 2.5kg plate
grid and both cost the same four weeks. A stall therefore has a fixed price
regardless of which reset is in play.

The second reset is the genuine slowdown: +2.5kg weekly is baseline rate off a
lower start, whereas +5kg weekly is faster than baseline and functions as a
bounce.

**A reset suspends the ladder rather than altering the anchor.** The drop is
taken from the failed load, the re-climb runs at the reset's own rate, and when
the sequence reaches the failed load again the ladder resumes from where it left
off. The anchor is untouched, because the anchor is a measurement of where the
block started and a stall is not evidence about that.

**A resume spends the stall.** After a re-climb returns to the failed load, the
next miss there is a first miss again — otherwise the second stall would fire
immediately on arriving back and the reset would have bought nothing.

Worked example from a 90kg failed load, on a 2.5kg plate grid:

| Week | Load | Result |
|---|---|---|
| 1 | 90 | miss |
| 2 | 90 | miss → reset 1 |
| 3 | 80 | pass |
| 4 | 85 | pass |
| 5 | 90 | miss |
| 6 | 90 | miss → reset 2 |
| 7 | 85 | pass |
| 8 | 87.5 | pass |
| 9 | 90 | miss |

**Corrected 2026-08-19, and the arithmetic is the reason.** This table used to run
to eleven weeks, with the second reset dropping to 80 and re-climbing 82.5, 85,
87.5, 90 — a 10kg drop at +2.5kg a week, which is four re-climb weeks and not the
two the first reset costs. The stated protocol is −5%, which from 90 is 85.5 and
lands on 85. So both resets cost two weeks, which is what "chosen as a pair so both
cost the same" says two paragraphs above, and the old rows contradicted the table
they were illustrating. `crates/domain/tests/progression.rs` asserts this version
load for load.

**A third stall has no protocol and therefore holds**, re-issuing the failed load
until it goes up. That is the absence of a decision rather than a decision, and it
is what the code does so the state stays legible.

## Blocks and testing

Block length is bounded by the nutrition phase: 12 weeks maximum for a deficit,
8 for maintenance. Every block contains at least one test, leaving 11 or 7
climbing weeks.

The test is unconditional. A block ending mid-re-climb requires no special
handling — the test runs regardless.

A test coming in below the ladder's final week is the **expected** outcome after
stalls, not a failure signal. It confirms what the stalls already implied. Where
the ladder got to is an intention, and the test says how much of it was real.

Because the test replaces the anchor, the stall count does not need resetting.
Its subject no longer exists.

## Deliberately undecided

- **Third stall.** No protocol. Within an 11-week ceiling, three stalls do not
  fit before a test intervenes, so the case is not reachable in practice. If it
  becomes reachable, the alternation model already offers a candidate response —
  a persistent stall near the ceiling is the stated switch point to a hinge
  block.
- **Anchor carry-across block boundaries.** Not specified; the test at the
  boundary makes it mostly moot, but the case of a block ending before its test
  has not been examined.
- **Interaction between reset cost and block runway.** A stall costs four of 7
  or 11 climbing weeks. This is a real constraint on what a maintenance block
  can achieve, and it belongs in the periodisation model rather than here. It
  does bound how far a block gets: a block leaving no room for one reset cannot
  survive a stall within its own weeks.
- **Descending repetitions across the block.** The textbook linear block does
  this; § The plan records why `linear` does not.
- **Fractional plates.** 2.5kg is the smallest available increment, so the climb
  rate is slowed by cadence rather than step size. 0.5kg pairs would remove that
  constraint. Not proposed.

## Evidence requirements

The gate is negative — the plan proceeds by default and retreats on evidence.
This requires the performed record to distinguish **trained and failed** from
**did not train**. A failed attempt must be recorded as an attempt.

The logging app supports this: Hevy records a failed attempt as zero
repetitions. The discriminator is the rep count and not the `failure` set type,
which means "taken to failure" and appears on 77 completed sets in the record
against one genuine failure. Translating it is specified in
`specs/003-prescribed-workout-generation`.
