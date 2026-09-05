# 0035 — A subset is chosen by what it keeps, not by where it is cut

**Date**: 2026-09-05

**Amends** `0032-a-provider-is-asked-for-a-shape-and-keeps-its-own.md`, whose
answer for *Power Zone Build* at four microcycles of two sessions changes from
**µ2–5** to **µ1, µ2, µ4 and µ5**. 0032's own framing survives intact — the
provider keeps its shape and answers a request — and so does its method. What
changes is the criterion that chose the microcycles, which was keyed on a
measure that mistook Build's opening week for a deload.

**Does not disturb** `0034-programmes-cohere-at-their-troughs.md`. Checked: 0034
needs only that Build's answer ends in a deload and that the deload is µ5. Both
hold for µ1-2-4-5, so every gym test week still lands on a cycling deload and the
autumn calendar is unchanged.

**Scope**: how a sub-shape is selected from a published programme.

## What was decided

**A selection need not be contiguous.** The operator, 2026-09-05:

> "I don't see any reason why we have to take contiguous microcycles once we've
> decided we're not going to complete the programme as written."

**But the written order is always kept.** Microcycles may be dropped, never
reordered — so the search is over subsets and never over permutations, and no
score here has to be order-aware.

**The last microcycle is forced by what it carries, not by its position:**

> "We must have microcycle 5 so that our final week is a test week, so really
> we're comparing 1-2-3, 1-3-4, 1-2-4, and 2-3-4 for the non test weeks"

That is 0034's rule reaching inside a programme: a mesocycle ends in a test or a
deload. For Build µ5 is the only microcycle at the bottom level, so it is
mandatory; for an eight-microcycle programme two positions qualify and neither
is.

## Why µ2–5 was wrong

**`hard_share` cannot tell Build's opening week from its deload.**

```text
µ1   111 min   intensity 76.4   TSS 108   Z4+ 0%
µ5    64 min   intensity 76.8   TSS  63   Z4+ 0%
```

Identical intensity, 74% more riding, and both report 0% time at zone four and
above. 0032's 3:1 rule was keyed on hard share, so it read µ1 as a deload, so
µ1-4 looked like "deload, then a rise" and was rejected — leaving µ2-5 as the
only contiguous run ending in one.

**This is the same blindness issue #71 opened on**, showing up inside a
programme rather than across one. *Boost Your Base* is eight microcycles of zeros
to hard share; Build's µ1 is one. Under TSS, or under volume and intensity read
apart, µ1 is plainly a full-volume endurance week that happens to contain no
threshold work.

**And the operator's own reading of the arc says which to drop:**

> "From the TSS, it looks like micros 1-4 have a shape like 1-2-2-3, so it makes
> sense that we get the best fit by dropping micro 2 or 3."

At sessions 1+3 the working arc is 76, 90, 92, 102 — µ2 and µ3 are 2.2% apart,
one level. Dropping either removes the duplicated level rather than a step.

## How a candidate is judged

**Two structural checks first. They refuse; they do not score.** That is 0034's
shape, applied here.

1. **It must end at its bottom level**, with something above it —
   `cycling::shape::mesocycles`.
2. **It must not drop a zone the programme trains** —
   `cycling::shape::zones_lost`. Build's µ1-2-3 loses every second of Z6 and Z7,
   all of its anaerobic and neuromuscular work.

**Then two scores, both length-blind, because a three-microcycle selection is
being compared with a four-microcycle programme.** Neither compares the arcs
point by point: that would need them resampled to a common length, and § II
names resampling as a mistake.

- **Composition** — `diverges`, 0032's own method, summed absolute percentage
  points across the seven zones. Length-blind because it normalises to shares.
- **Span** — hardest working microcycle over easiest. A ratio, so the number of
  microcycles does not enter it. Build's written working span is 1.30×.

```text
subset      composition   span    zones lost
µ1-2-3            16.9    1.21    Z6, Z7        refused
µ1-2-4             5.4    1.34    —
µ1-3-4            13.0    1.34    —
µ2-3-4            22.7    1.13    —             0032's answer
```

**µ1-2-4 wins.** The operator:

> "we'll always keep the same order as was written, so it sounds like 1-2-4 is
> the winner. As I said, we have to loose something and that appears to be the
> subset that looses the least."

## Two things this agent proposed and withdrew

**Evenness — smallest step over largest — was offered as a third length-blind
score and is not one.** The written programme has three steps where a
three-microcycle selection has two, and more steps means more chances of a small
one, so evenness falls with length for arithmetic reasons rather than musical
ones. Worse, it is circular here: µ2 and µ3 being one level *is* the small step,
so dropping either must improve evenness. It measured the operator's own
observation back at him.

**Chi-squared was proposed as a better aggregation of composition, accepted on
that basis, and then withdrawn as wrong.** The claim was that dividing each
zone's squared error by that zone's own share would make half a rare zone cost
like half a rare zone. It does not. For a zone of reference share `e`:

```text
candidate has          summed |pp|   chi-squared
none of it                   e            e        identical
half of it                   e/2          e/4      the squared form charges less
ten times it                36          324        and far more the other way
```

**It is the *less* sensitive of the two to a zone going missing**, and is only
harsher on a rare zone being over-represented — where on Build's data it blows
up on Z7 at 0.09% against 0.05%, a difference of a few seconds of riding.
Proportional error `|c − e| / e` does do what was claimed, and is dominated by
that same Z7. So `diverges` is left exactly as 0032 had it, and the fact none of
them can see is carried by `zones_lost` instead.

**The choice of measure is not load-bearing here.** Summed, squared and
proportional all rank µ1-2-4 first and µ2-3-4 last; only second and third place
move. The conclusion does not rest on which was picked, which is the only reason
picking it wrongly did no damage.

## What it costs

**Dropping µ3 halves the mesocycle's VO2 work, and does it in the worst place.**

```text
µ2 s1   Z3 13:00  Z4 14:00                     a threshold week
µ3 s1   Z4 10:00  Z5 11:00                     where VO2 is introduced
µ4 s1   Z4 3:45  Z5 8:00  Z6 5:00  Z7 0:15     the Max Ride
```

With µ3 gone the mesocycle carries 8 minutes of Z5, all of them inside the Max
Ride itself, so µ4 is the first VO2 exposure of the block. Keeping µ3 and
dropping µ2 instead arrives at the Max Ride with 11 minutes behind it.
`µ1-3-4` is that option, and it costs 13.0 in composition against 5.4.

**No score here can see it**, and none ever will: every one of these measures
treats a mesocycle as a bag of minutes, and this is a question about order.
Preserving the written order means the only order question left is *which weeks
are dropped*, and that one is answered by judgement — the operator's, recorded
above as a trade accepted rather than a cost avoided.

**Non-contiguity is a real departure from a published programme.** Dropping the
first microcycle is a programme started later; dropping a middle one is a
programme with a hole in it. 0029 says the provider owns its shape, and this
takes rather more of that shape apart than 0032 did.

## Consequences

- 0032's answer of sessions 1+3 is unchanged and was never in question — it wins
  under every measure tried, over every microcycle subset.
- `is_three_to_one` was the vehicle for the mistake this corrects, and is
  already gone (0034, amended).
- The search is in `infrastructure/examples/transcribe.rs`, which now takes the
  request as arguments — `transcribe skeleton.txt 4 2` — and answers Build with
  µ1-2-4-5 by sessions 1+3 at 5.4. It refuses eight of its fifteen candidates
  before scoring any of them.
- **The structural checks change the answer rather than tidying it.** Within
  µ2-3-4-5, composition alone prefers sessions 2+3 at 18.2 over 1+3 at 22.7 —
  and 2+3 stops training Z6 and Z7, because session 1 carries the Max Ride. The
  same is true of every 2+3 candidate in the programme. A score that had the
  dropped zones folded into it would have chosen one of them.
- ~~It does not yet generalise to a programme of two mesocycles.~~
  **Answered by 0036 the same day, by dissolving the question.** Asked for four
  by two, Peak answered µ3-5-7-8 and Base µ2-6-7-8 — selections straddling both
  halves of themselves. That was not a missing constraint but a malformed
  request: **a provider supplies mesocycles, not programmes**, so an
  eight-microcycle programme is two answers and never one. Split first and there
  is no boundary left to cross. Everything above stands — Build is one mesocycle
  of five, which is why it was the only programme this worked on.
