# 0031 — The instruction rounds down and the inference rounds up

**Date**: 2026-09-04

**Departs from the published programme.** SBS's notes say to round the inference
down. This rounds it up, and the reason is that their own rule loses ground on a
perfect execution.

**Scope**: `sbs::chart::advance`, and by contrast `sbs::chart::working_load`.

## What was decided

**The two roundings are opposite, because they are different acts.**

```text
working_load   maximum → a load to put on the bar     floors
advance        a load lifted → a maximum              raises
```

`working_load` floors because it names a weight the operator must lift, and
asking for more than he has shown is the one error a prescription must not make.
`advance` raises because it infers a maximum from a load **that was already
floored** — rounding down a second time compounds the instruction's own rounding
into the model that generates the next instruction.

## Why

**The maximum makes a round trip through the plate grid every repetition-maximum
day.** Down through `working_load` to a load, back up through `advance` to a
maximum. Rounding down at both ends loses at both ends, and the division
amplifies the first loss by `1/share`.

Flooring both, doing **exactly what the chart asked**:

```text
training max        start   cycle 1   cycle 2   cycle 3
                     92.5        85      77.5        70

the 8RM day asks    72.5kg    67.5kg      60kg
```

The programme ran backwards on a perfect execution — 22.5kg of training maximum
lost across the autumn by meeting every target.

**The programme notes have the same flaw, and their example hides it.** They
say:

> "For a 10RM, count the weight you get as 75% of your training max for the next
> week. For example, if you have a training max of 300 going into the week, and
> you get 235 for a 10RM, increase your max to 235/.75=313. When it's between two
> 5-pound increments, round down. So in this case, you'd use 310."

The operator, 2026-09-04:

> "I don't think they've thought this through. their example works because
> they've chosen a round number. 300 \* 0.75 = 225, 225 ÷ 0.75 = 300, no problem.
> 295 \* 0.75 = 221.25, which rounds down to 220. 220 ÷ 0.75 = 293.33, which
> rounds down to 290. So, I think your option 1 is the right answer, floor the
> instruction, ceil the inference."

Five pounds a week, on a lifter who does everything asked. Their worked example
is stable only because 300 is exactly divisible by the increment at that share;
no other number is.

**Why this surfaced now.** Until 2026-09-04 the repetition-maximum top set was
prescribed as a work-up with no load, so the expected case was *exceeding* an
unstated target and the maximum went up. Once the day states its load, meeting it
exactly is the expected case — and meeting it exactly was the case the arithmetic
punished.

## What was rejected

**Clamping the maximum at its previous value** — `max(old, inferred)`. It holds
on an exact hit and rises on a beat, but it also holds on a *miss*: a
repetition-maximum day the operator falls short of would leave the maximum
untouched. Worse than the fault it fixes.

**Rounding the target to nearest instead of flooring it.** Settles the drift
after one 5kg loss, but prescribes a load the operator has not shown he can hold
— which is the whole reason `working_load` floors.

**Leaving it.** Defensible only if hitting the target exactly were rare, and the
change on 2026-09-04 made it the expected case.

## What it costs

**We are no longer running the published arithmetic**, and a note in the code
must say so or a later session will "fix" it back. The chart, the shares and the
progression mechanism are all still SBS's; one rounding direction is ours.

**A missed target now costs more than it did.** Falling one increment short of
the 8RM target takes the maximum from 92.5 to 80 over a cycle, against 75 under
the old arithmetic — the drift that used to mask a miss is gone, so a miss reads
at its true size. That is the correct behaviour and it will look harsher.

## Consequences

- `advance` raises to the increment; `working_load` is unchanged.
- `meeting_every_target_holds_the_maximum` pins the property over three cycles,
  and `the_maximum_still_moves_with_the_performance` pins that holding on a hit
  did not come at the cost of moving on a beat or a miss.
- Six expected values in `domain/tests/sbs.rs` moved. Every one of them was the
  old arithmetic written down, not an independent check of it.
