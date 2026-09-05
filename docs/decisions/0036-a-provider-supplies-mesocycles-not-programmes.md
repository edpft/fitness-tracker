# 0036 — A provider supplies mesocycles, not programmes

**Date**: 2026-09-05

**Amends** `0035-a-subset-is-chosen-by-what-it-keeps.md`, whose closing open
question — whether a selection may cross a mesocycle boundary — turns out to be
malformed rather than open. Nothing else in 0035 moves: Build's answer is still
µ1-2-4-5 by sessions 1+3, and the checks and scores that chose it are unchanged.

**Overrides** `0029-a-provider-is-the-context-and-the-planner-asks-it-for-a-shape.md`
on one point: it asked that an answer be recorded as a **set** of options where
more than one scores well. It is a single answer.

**Scope**: what a provider is asked for, and what it hands back.

## What was decided

**The unit of supply is a mesocycle.** The operator, 2026-09-05, on why Peak and
Base kept answering a four-microcycle request with selections straddling both
halves of themselves:

> "that's a product of asking programmes of 2x 4 microcycle mesocycles to give
> you 1 4 microcycle mesocycle that represents the entire programme. essentially,
> we're asking Build for a 4 microcycle mesocycle because we're dealing with a
> grid of 4 microcycle mesocycles. Build is a 5 microcycle mesocycle, so we need
> a way to shrink it."

> "I think what we actually have in the power zone programmes is:
> base 1, micros 1-4 / base 2, micros 5-8 / build, micros 1-5 /
> peak 1, micros 1-4 / peak 2, micros 5-8"

**Five mesocycles, and only one of them needs shrinking at all.** Four are
already four microcycles; Build is five. The whole apparatus of subsets and
structural checks in 0035 exists for that one case, which is why Build was the
only programme it worked on.

### The decomposition was not given to the code — it falls out

Take the shortest prefix that is a mesocycle, then start again after it.
`cycling::shape::partition`, and it reproduces all five from TSS alone, under
every two-session selection as well as all three:

```text
Base    µ1-4, µ5-8
Build   µ1-5
Peak    µ1-4, µ5-8
```

That is corroboration rather than construction: the operator named the five from
training knowledge and the criterion of record found the same five from the
numbers. Nothing here was fitted to make them agree.

### And the mesocycle is demonstrably the unit, not the programme

**The best session pair differs between two mesocycles of the same programme.**
Base's first answers with sessions 2+3 and its second with 1+2. So "which two
sessions of Base" has no answer, and asking the question of a programme rather
than of a mesocycle was the mistake.

## The answer is the lowest score, and nothing more

The operator, 2026-09-05:

> "To start with, I think we just pick the smallest value. 1.1 and 1.2 are so
> close it doesn't matter, which means it doesn't matter which one we pick, so we
> might as well pick 1.1. 5.4 is smaller than 6.8, so we pick 5.4, no need for
> anything more complicated."

**That argument dissolves 0029's set rather than deferring it.** A set exists to
preserve a choice; if the scores are close enough to be a tie then there is no
choice being preserved, and if they are not close then there was never a set. No
threshold is needed, and this agent had been about to ask for one.

**What it gives up, and why that is not urgent.** A runner-up could still be
preferred by something the composition score cannot see — spacing, which session
carries a test, or what the gym is doing that week, all of which 0032 already
recorded as outside it. If a downstream constraint ever needs to reject the
winner, the set comes back. "To start with" is the operator's own hedge and is
recorded as stated.

## What the five mesocycles answer, at four microcycles of two sessions

Read from the Peloton API on 2026-09-05 and computed by
`transcribe <skeleton> 4 2`:

```text
mesocycle   answers                    composition   span
base 1      µ1-2-3-4  by sessions 2+3          6.0   2.12×
base 2      µ5-6-7-8  by sessions 1+2          1.1   1.04×
build       µ1-2-4-5  by sessions 1+3          5.4   1.34×
peak 1      µ1-2-3-4  by sessions 1+3          5.4   1.16×
peak 2      µ5-6-7-8  by sessions 1+3         14.6   1.32×
```

The autumn needs three of these. Both pairings 0034 admitted are three:
`base 1, base 2, build` and `build, peak 1, peak 2`.

## What it costs

**A programme that contains no deload anywhere decomposes into nothing**, and is
therefore unusable rather than merely unusual. That is the right failure — a
run of rising microcycles is a progression and forcing a split would invent a
deload the programme does not contain — but it means a provider can answer
nothing at all, and the planner must handle that rather than assume a shape.

**A tail that is no mesocycle is dropped silently by `partition`.** The ranges it
returns need not cover the programme. Nothing in the three programmes read so far
has such a tail, so the behaviour is untested against real data.

**The greedy rule is shortest-first and is not defended as optimal.** It could in
principle take a short mesocycle early and strand a longer one after it. It does
not on any programme read, and the alternative — searching every partition —
would need a criterion for preferring one over another that nobody has stated.

## Consequences

- `cycling::shape::partition` and `is_mesocycle` join `bottom_level`,
  `mesocycles`, `span`, `diverges` and `zones_lost`. All take scores rather than
  rides, so the gym side can use them when it has a score of its own.
- `transcribe <skeleton> 4 2` splits the programme first and asks each mesocycle
  separately. It reproduces all five answers above.
- 0035's search is unchanged in substance; it now runs inside a mesocycle rather
  than across a programme.
