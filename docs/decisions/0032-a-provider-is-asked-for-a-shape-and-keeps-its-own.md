# 0032 — A provider is asked for a shape and keeps its own

**Date**: 2026-09-05

**Follows** `0029-a-provider-is-the-context-and-the-planner-asks-it-for-a-shape.md`,
which said the planner asks a provider for a shape. This says what the provider
does *not* do in answering: change what it is.

**Scope**: how a published cycling programme answers a request for a mesocycle,
and how the answer is computed.

## What was decided

**A programme's own shape is a fact about the programme. The answer to a request
is a different thing and does not replace it.**

Power Zone Build **is five microcycles of three sessions**. Asked for *four
microcycles of two sessions*, it answers **sessions 1 and 3 of microcycles 2 to
5**. Microcycle 1 is what it sheds to answer; it is not a lead-in that was
outside the mesocycle all along.

The operator, 2026-09-05, correcting this agent:

> "Build's mesocycle is 5 microcycles of 3 sessions but, if we ask it to provide
> a mesocycle of 4 microcycles of 2 sessions, then sessions 1 and 3 of
> microcycles of 2-5 is probably the right shape."

**Two rules produce that answer, and both were already in the record.**

**1. A mesocycle is three working microcycles and a deload — 3:1.** The
operator's, and the criterion by which SBS and Peak were found to coincide.
Applied to Build:

```text
microcycles 1-4    0% → 12% → 29% → 42%   peaks last — not a mesocycle shape
microcycles 2-5   12% → 29% → 42% →  0%   deload last — 3:1
```

Hard work is time at Z4 and above as a share of that microcycle's riding.

**2. The closest sub-shape is the one that diverges least in zone profile.** The
method transcribed for Peak (`docs/cycling-peak-your-power-zones.md`): express
each candidate as proportions of timed ride, and score it by the summed absolute
difference in percentage points across the seven zones against the programme's
own proportions.

```text
sessions 1+3    8.5    arc 17% → 27% → 44% → 0%
sessions 1+2   17.5    arc 22% → 51% → 45% → 0%
sessions 2+3   24.9    arc  0% → 14% → 37% → 0%
```

Sessions 1+2 doubles microcycle 3. Sessions 2+3 erases microcycle 2 entirely.

## Why the correction was needed

**This agent invented a second coincidence criterion instead of applying the
one that existed.** It claimed Build's microcycles 1–4 coincided with an SBS
block because both "build across four microcycles" — which is a description of
any progression rather than a structure, and which fails on its own terms the
moment 3:1 is applied, because Build's microcycle 4 is the hardest week in the
programme and not a deload.

**And it then redefined the programme to fit.** Saying "Build's mesocycle is
weeks 2–5, week 1 is a lead-in" makes the provider a function of what the planner
asked, which is precisely the direction 0029 forbids. The provider owns its
shape; the planner gets an answer.

**Microcycle 5's two endurance rides are what make the 3:1 real.** This agent
also asserted that microcycle 5 was the FTP warm-up and test and nothing else. It
is two 45-minute endurance rides *plus* the pair — real riding volume with no
Z4 in it, which is a deload. Without them microcycle 5 would be a test day and
Build would contain no 3:1 anywhere.

## Sessions are ordinal, not calendar days

The operator numbered Build's sessions 1, 2, 3 rather than the 1, 3, 6 used when
Peak was transcribed. **The ordinal numbering is the correct one**, and 0018
already says why: the programme counts cycles and the scheduler owns the
calendar, so a programme naming weekdays is a programme doing the scheduler's
job.

What it costs is that "sessions 1+3" carries no spacing, where "days 1+6"
implicitly did. Spacing is a genuine fatigue input — but it is the allocator's,
resolved against the operator's slots, and not the provider's to state.

## What it costs

**The divergence score is a heuristic and is not defended here.** It ranks
candidates by zone profile alone: it knows nothing about spacing, about which
session carries the test, or about what the other discipline is doing that week.
It picked sessions 1+3 decisively, and the weekly arc agrees, and the operator
agrees — but three agreeing weak signals are not a proof.

**"Probably the right shape" is the operator's own hedge** and is recorded as
stated. It is the answer to build against, not a settled fact about training.

## Consequences

- `docs/cycling-power-zone-build.md` is the transcription, read from the API
  rather than from screenshots.
- The planner asks a provider for `(microcycles, sessions per microcycle)` and
  receives a shape; 0029 wants that answer recorded as a **set** of options
  where more than one scores well. For Build at four-by-two, one option is
  decisively ahead and the set has one member.
- The 3:1 rule is now the coincidence criterion of record. Whether SBS and Build
  cohere is a question to be answered with it, and has not been answered here.
