# 0030 — The work-up to a rep max descends from its own rep count

**Date**: 2026-09-03

**Answers** the first half of the handover's open question 3, the rep-max
work-up ramp. It supersedes that question's premise: there *is* a published
protocol, and the earlier research missed it.

**Scope**: the ramp before a repetition-maximum top set. SBS's day 2 is one of
these every week — `1×8 @ 8RM`, `1×5 @ 5RM`, `1×3 @ 3RM`, `1×1 @ 1RM` — so this
runs twelve times across the autumn's three cycles.

## What was decided

**The warm-up rep count is a function of the top set's rep count, floored at the
ramp already in use.**

```text
reps = max(floor, descent)      floor   = 4, 3, 2, 1
                                descent = n, n, n−2, n−3
```

The loads are unchanged: 40%, 60%, 80%, 90% of the day's anchor.

```text
8RM   8, 8, 6, 5        descent
5RM   5, 5, 3, 2        descent
3RM   4, 3, 2, 1        floor — descent would reach 0
1RM   4, 3, 2, 1        floor — descent would go negative
```

**The floor is not a special case bolted on.** For `n ≥ 4` the descent is at or
above the floor at every position, so the two compositions — an element-wise
maximum, or "floor below `n = 4`, descent above" — agree everywhere. The
element-wise form is stated because it is total without a branch.

**Only the 5RM and 8RM days change.** The 3RM and 1RM days keep the ramp the
record already shows: `4@35 3@52.5 2@70 1@77.5` and `4@37.5 3@55 2@75 1@82.5`.
That is a property worth noticing rather than a coincidence — the existing ramp
was already right for the rep counts it was being used at, and wrong only where
it had never been asked to reach.

## Why

**Greg Everett, Catalyst Athletics, 13 November 2017**, *How Do You Warm Up For
A Weightlifting Exercise?* — the operator found it. On warming up to a rep max:

> "you can reduce the number of reps in your warm-up sets as you're working up
> to allow you to get warm but minimize fatigue"

> "This might mean doing 1-2 sets of 5, then sets of 3 and 2 until getting to
> your 5 rep weight."

And on the load side, which is why the percentages did not move:

> "Use the same progressively smaller weight increases on the way up that you do
> in any other warm-up"

40/60/80/90 has gaps of 20, 20 and 10, which is that instruction already.

**The operator, on adopting it**: *"I've got more faith in Greg Everett opinion
than my own but I like that mine was going in a similar direction."* His one
amendment was the 8RM shape: *"I'd say the 8rm shape should be more like
8,8,6,5."*

**The two statements are the same rule, and neither was fitted to the other.**
Everett's published 5RM example is `5, 5, 3, 2`. The operator's 8RM is
`8, 8, 6, 5`. Both are `n, n, n−2, n−3`. One is a coach's worked example from
2017 and the other is the operator's judgement about a lift Everett was not
writing about, and they agree on all four positions. That is the corroboration
this ramp rests on.

## What was rejected, and by whom

**`n+1, n, n−1, n−2`** — the operator's own first proposal. It opens *above* the
top set's rep count, which Everett's rule never does, and it is undefined at
`n = 1` (the sequence runs 2, 1, 0, −1). Withdrawn by him once the article was
in hand.

**A flat cap of six reps.** Proposed by the operator as a way to stop the ramp
running long, then overtaken. Everett's shape achieves the same end better: a
cap takes reps off the light sets, where nine front squats at 30kg cost nothing,
and the fatigue that matters is at the top.

**`8, 8, 3, 2` for the 8RM** — this agent's reading of Everett. Too steep. The
operator's `8, 8, 6, 5` is what stands.

**Everett's max-single sequence** (`35%×2×3, 48%×2, 62%, 76%, 83%, 90%, 94%,
97%`) is **not adopted.** Nine sets rising to 97% before a maximal single is a
weightlifting habit — a snatch warm-up is largely technical rehearsal, where a
front squat is a grind. Nothing about the front squat asked for it and the
operator did not ask for it.

## What it costs

**The 8RM day's warm-up is 27 reps, and the last of them is 5 at 70kg.** That is
close to what the uncapped `n+1` proposal would have cost, and it is a real
amount of front squatting before the set that matters. The operator chose it
over this agent's lighter reading with the volume in front of him.

**The rule rests on two points.** Everett publishes one worked example, at
`n = 5`; the operator supplied `n = 8`. `n, n, n−2, n−3` is the rule that fits
both, and the floor covers `n ≤ 3` — but no source states the general rule, and
a third data point could contradict it. It is an interpolation between a
published example and the operator's judgement, and should be described that way
rather than as Everett's protocol.

## Consequences

- `WarmupStep` carries `of_top_set` and a fixed `reps`, so the rep count can no
  longer be a stored constant: it is a function of the top set's rep count. This
  is what 0028 anticipated when it took `WarmupStep` out of the generation
  parameters.
- The four SBS rep-max days each derive their own ramp; nothing new needs
  stating per cycle.
- The anchor each ramp is a percentage *of* is a separate matter and is not
  settled here — see the handover's note on the 8RM anchor, which the record
  suggests is too heavy.

## Source

Greg Everett, *How Do You Warm Up For A Weightlifting Exercise?*, Catalyst
Athletics, 13 November 2017.
<https://www.catalystathletics.com/article/2102/How-Do-You-Warm-Up-For-A-Weightlifting-Exercise/>
