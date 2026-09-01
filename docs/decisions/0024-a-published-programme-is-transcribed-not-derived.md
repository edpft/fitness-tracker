# 0024 — A published programme is transcribed, not derived

**Date**: 2026-09-01

**Supersedes**: `0023-a-block-derives-its-loading-and-discovers-its-result.md`
as the plan for the autumn block. 0023 is not wrong and is not withdrawn; it is
parked. See *What happens to the Prilepin block*.

**Retires**: `specs/`, and the `speckit-*` skills with it.

## Context

The operator's direction on 2026-09-01, in his words: *"time for a different
approach, I still want to focus on introducing concurrent gym and cycling
training but I want to lean more heavily on published programmes instead of
trying to build our own."*

This reverses the shape of 0023 and not its reasoning. 0023 recorded him
rejecting a transcribed table — *"I don't want you to copy what they were doing,
I want you to try and extract the why"* — and that was about his old gym's **BTWB
export**, which is a record of what happened rather than a statement of intent.
Stronger By Science's `28 Training Programs` is a published programme with named
authors, a rationale document and a spreadsheet. Adopting one is a different act
from reverse-engineering the other, and the earlier rejection does not reach it.

**What did go wrong in 0023 is a fact about the output, and the operator named
it**: *"it had gotten to the point that we were prescribing 8x 2, which is
insane, just to keep with the Prilepin bands."* 0023 anticipated this in its own
open question — the repetitions descending one per week from six *"degenerates on
long blocks — a ten-week block runs `8 × 2` three weeks running as the band's
floor holds it"* — and recorded that nobody owned the rule that picked. The
degeneracy was documented as unresolved and then prescribed anyway.

## Decision

**1. `SBS` is a new programme type.** Not a `Block`, not a `Linear`.

```text
Programme  ─┬─ Test                    one week, no ladder, a maximum
            └─ Periodisation ─┬─ Linear   a top-set ladder at a rate
                              ├─ Block    phases derived from Prilepin (parked)
                              └─ SBS      a transcribed published chart
```

The operator named it. It is not "a block with the derivation switched off": its
loads are read off a published table rather than computed from a chart, and the
thing that moves week to week is the maximum itself rather than a position on a
ladder.

**2. The programme is the SBS 2×/week intermediate squat routine, with the
front squat on both days.** The published routine runs a back squat on day 1 and
a front squat on day 2; the operator runs the front squat on both. Eight
prescriptions, verified against `Squat 2x Int` in the workbook:

| week | day 1 | day 2 |
|---|---|---|
| 1 | 5×5 @ 80% | 1×8 @ 8RM, then 3×5–6 @ 8RM |
| 2 | 4×3 @ 85% | 1×5 @ 5RM, then 3×3–4 @ 5RM |
| 3 | 3×1 @ 90% | 1×3 @ 3RM, then 3×1–2 @ 3RM |
| 4 | 3×3 @ 75% | 1×1 @ 1RM |

**Week 4 day 1 is the operator's, and it is a transposition rather than an
invention.** The published intermediate week 4 day 1 is nothing at all — the
routine goes straight to the test. He took the *beginner* sheet's week 4 day 1,
which is `3×3 @ 70%` against a week 1 of 75%, and moved it to sit 5 points below
the intermediate's week 1 of 80%. The taper is the published one; only the
reference point changed.

**3. The maximum moves inside the cycle, and SBS's own table moves it.** After
each rep-max day, the weight achieved is counted as a share of the maximum the
*next* week programmes from:

| | SBS | the domain's `repmax.rs` (RTS) |
|---|---|---|
| 8RM | 80% | 82.5% |
| 5RM | 85% | 90% |
| 3RM | 90% | 95% |

**These stay separate, settled by the operator on 2026-09-01.** They answer
different questions. `repmax.rs` estimates a one-rep maximum from what was
lifted; SBS's table decides what to programme from next week, and its generosity
is the mechanism — it is how the bar goes up weekly without a test. SBS's three
numbers live with the SBS programme. `repmax.rs` is untouched.

The divergence is not small and runs the way round that surprises: a 100 kg
triple implies a 105.3 kg maximum under the domain's table and a 111.1 kg one
under SBS's. The workbook confirms there is no conservative discount hiding
anywhere — the `Maxes` sheet holds one number labelled `MAX`, applied
undiscounted, over the note *"use whatever units you want"*.

**4. Week 4 runs first, standalone, to open on a measured maximum.** The
operator: *"we will use week 4 as the stand alone test week so that we can start
with a 1RM, then we will programme in blocks of 4 weeks."*

So the cycle is self-perpetuating and the model already has the pieces:

```text
week 4 standalone   Programme::Test — 3×3 @ 75%, then a 1RM
                    the taper is a share of the maximum standing at the time
                    (the summer block's), which is what 0016.5 already says a
                    standalone test's light session does
4-week SBS block    weeks 1–4; week 4 day 2 is the exit test, and its 1RM is
                    the next block's opening maximum
```

**5. A percentage inside the cycle is a share of the maximum current at that
week**, not of the maximum the cycle opened on. So week 4 day 1's 75% is a share
of the maximum week 3's triple just set. It is a taper in percentage terms and
may not be one in kilograms, and that is intended.

**6. Kilograms, floored to 2.5.** The workbook's own rounding, and `FLOOR` is
its own function — a load is rounded down to the increment, never up.

## What happens to the Prilepin block

**Parked, not deleted.** `feat/block-derives-from-prilepin` is finished and
green and was never raised as a pull request. It stays unraised. The operator:
*"the Prilepin block started with good intentions, and probably has some good
ideas."*

Two things follow. The open question 0023 left — which repetition count to pick
inside Prilepin's band — **is no longer open, because nothing asks it.** And the
`WorkUp` variant 0023 added to `WeekPlan` is exactly what SBS's rep-max days
need, so the one piece of it that the new path depends on is already written.

## Why `specs/` goes

The operator: *"fuck the specs, they generated loads of text that I never read
and, it turns out, have been making you push against the thing that I actually
want, which is concurrent programming."*

He is describing something that had just happened. Asked where cycling stood, a
session quoted `specs/003-prescribed-workout-generation/spec.md` — *"Out of
scope: Cycling, nutrition phases and the constraint calendar"* — back at him as
evidence. That line was written by an agent in August as a scoping decision for
one feature. It was read in September as a constraint on what the operator is
permitted to want, from the tool he is paying for and specifying.

**8,244 lines across 31 files, none of which he has read.** Nothing load-bearing
is only there: 630fa02 moved the seed, the candidates and the vocabulary into
`domain`, which was the last of it. What the decisions cite from it is
provenance — "raised by 002, question Q1" — and git history keeps that reachable.

The general rule, which is not new and is the reason this is a decision rather
than a tidy-up: **the repository cannot corroborate the agent that wrote it.**
The constitution is the operator's. `docs/decisions/` records what he settled.
Everything else in prose — specs, module documentation, roadmap commentary — is
an agent's own writing, and quoting it at him is citing oneself.

`specs/` and `.claude/skills/speckit-*` are pending deletion; the bulk removal
was refused by a permission classifier on 2026-09-01 and awaits the operator.
`CLAUDE.md` withdraws their authority in the meantime, which is the part that
changes behaviour.

## Open, and blocking the half that matters

**The cycling programme's content.** The operator has chosen Peloton's *Peak
Your Power Zones*, sessions 1 and 2 of each week only, since he trains twice.
`members.onepeloton.co.uk` is behind his login and no agent can reach it. Needed:
the class list, and how many weeks it runs, so it can be laid against a 4-week
squat cycle.

**Credentials are not the answer to that** and were briefly assumed to be. The
operator asked how to hand them over without them reaching an Anthropic server;
the answer is that he does not hand them over at all. Nothing typed into a
session stays off the API. The class list is not a secret and can simply be
pasted. If a Peloton adapter is ever built, its credential arrives the way
Hevy's does — `PELOTON_API_KEY` in the environment, env-only with no flag,
per § 35 and `cli::catalogue` — and is never seen by an agent or a transcript.

**What a cycling prescription delivers.** Settled far enough to build:
*"cycling prescription can degrade to a calendar entry with a link to the
class."* So cycling is a discipline with a source and no writable sink, which
`cli::catalogue` already anticipates in shape and has never had an instance of.

**How the two disciplines share the calendar** — four sessions a week on
separate days, or two days carrying both — is unanswered and is the question
that makes this concurrent programming rather than two programmes that coexist.
