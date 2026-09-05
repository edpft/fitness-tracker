# 0034 — Programmes cohere at their troughs

**Date**: 2026-09-05

**Follows** `0032-a-provider-is-asked-for-a-shape-and-keeps-its-own.md`, which
settled what one provider answers when asked for a smaller shape. This settles
when two providers' answers may be run together — the question the planner
exists to answer, and the last one standing before it can be written.

**Uses** `0016-a-programme-is-a-test-or-a-periodisation.md`, whose `Test` — one
week, no ladder, a maximum — turns out to be what makes the two disciplines the
same length.

**Scope**: whether a gym mesocycle and a cycling mesocycle may be run
concurrently, and what decides it.

## What was decided

**Coherence is a constraint, not an objective.** The operator, 2026-09-05:

> "looking at this, I don't think INOL is useless, but I think the main thing
> that fell out of this analysis is that the troughs have to aligned: a test week
> in one should be a test week or relative deload in the other"

**The rule: a test week in one discipline must coincide with a test week or a
bottom-level microcycle in the other.** Nothing is optimised and nothing is
ranked. Arrangements are admitted or refused, and among those admitted the
choice is the operator's.

### The framing this replaces, and why it was wrong

This agent had put the question as a choice between two objectives — the
operator's own earlier framing, which he offered as alternatives rather than as
an answer:

> "we could want their peaks and troughs to align, so that one is hard when the
> other is hard, or we could want them to alternate, so that one is hard when
> the other is easy."

Neither is what the programmes do. **An SBS cycle has two peaks in different
weeks and a cycling mesocycle has one**, so "align the peaks" has no unambiguous
referent: SBS's volume peak is week 1 and its intensity peak is week 3. Laid in
phase, each finds a different partner — the volume peak sits on the cycling
mesocycle's opening and the intensity peak on the cycling peak. The operator's
reading, and the one this record is built on:

> "It sounds like these programmes have complementary shapes, which is what we
> want, not necessarily that they align perfectly. what we don't want is probably
> easier to identify than what we do"

**A rule keyed on the trough is more robust than one keyed on the peak, and that
is checkable rather than aesthetic.** The trough is the same week under every
metric tried — INOL, top-set intensity, TSS and Z4-and-above share all put an
SBS cycle's at week 4 and a cycling mesocycle's at its fourth. The peak is not:
INOL says SBS peaks in week 1, top-set intensity says week 3. A peak rule
inherits that disagreement; a trough rule does not.

This agent first wrote the rule as *"a test must not land on the other's hardest
week"*, which is the negative of the operator's and strictly weaker. It passes
arrangements his refuses — a cycling block opened one week early clears it at
every week but one, and fails his at four.

## A shape is levels, not ranks

The operator, 2026-09-05, on how these profiles are read:

> "Peak µ1-4 are clearly a 1-2-2-1 pattern, if the numbers were the other way
> around and they went 113, 126, 124, 114, they still would be"

> "Base µ1-4, is clearly a 1-2-3-1 pattern"

> "Peak µ5-8 is clearly a 2-3-4-1 pattern"

**Ordering within a level is noise.** 113 and 114 are one level whichever comes
first, so the shape survives their exchange. This corrected a worry of this
agent's that was the wrong worry: it had reported Peak µ4 as clearing µ1 by a
single TSS point and called the margin fragile, when both are simply at the
bottom and which is lower does not signify.

**Only the bottom level is derivable, and only the bottom level is used.** A
microcycle is at its mesocycle's bottom level when it is within a few per cent of
that mesocycle's lightest:

```text
                        above its mesocycle's floor by          bottom level
Peak µ1-4   1-2-2-1        1%     10%     12%      0%             µ1, µ4
Base µ1-4   1-2-3-1        2%     76%    127%      0%             µ1, µ4
Peak µ5-8   2-3-4-1      111%    116%    162%      0%             µ4
Base µ5-8                 47%     66%     87%      0%             µ4
Build µ2-5                95%    105%    124%      0%             µ4
```

**The tolerance is not a fitted number.** Everything at the floor's level sits
0–2% above it and everything a level up sits 10–162% above, so any tolerance
between 3% and 9% gives the same answer in all five mesocycles. Recorded at 5%,
in the middle of a fivefold berth.

**And the bottom level does not depend on which sessions are ridden**, which
matters because the operator rides twice a week and the figures here are the
three-session ones. Base, Peak and Build were all read from the API on
2026-09-05 and every mesocycle scored four ways — all three sessions, and each of
1+2, 1+3 and 2+3. **The last microcycle of every mesocycle is at the bottom level
in all twenty scorings.** The membership widens once — Peak µ1-4 taken as
sessions 2+3 reads 81, 82, 87, 81, which puts µ2 at the bottom too — and that
widens what the rule admits without disturbing what it requires.

**The full 1-2-3-4 labelling is the operator's reading and is not derived here.**
Peak µ5-8's 129 and 132 are two per cent apart and are levels 2 and 3; Peak
µ1-4's 124 and 126 are two per cent apart and are one level. Nothing in the
numbers separates those, and an algorithm claiming to would be one fitted to
three examples. The rule never asks which of two working microcycles is heavier,
so what cannot be derived is also not needed.

## Scoring, and what each metric is for

Only "bottom level" needs a number at all. A test week identifies itself by
containing a test.

**Cycling: Coggan's TSS from the zone plan** — `Σ (seconds × IF²) / 36`, in
`domain::cycling::ZoneProfile::tss`. Added because `hard_share` thresholds at Z4
and *Boost Your Base* contains none, reporting eight microcycles of flat zeros
for a programme that plainly has structure.

**The gym: INOL, and it is the operator's suggestion** — *"on a comparable
metric for the strength side, how about INOL?"* Hristov's
`reps / (100 − intensity)`, the published bands at
<https://liftvault.com/resources/prilepins-table-inol/>. It was the right
instinct for the reason TSS was: **it computes from the prescription alone**,
because `sbs::chart::training_max_share` already gives the repetition-maximum
days an intensity — 8RM at 80%, 5RM at 85%, 3RM at 90%.

```text
        day 1              day 2                  week      published band
w1   5×5 @80%   1.25   8RM + 3×5–6   1.15–1.30   2.40–2.55  loading
w2   4×3 @85%   0.80   5RM + 3×3–4   0.93–1.13   1.73–1.93  pre-peaking
w3   3×1 @90%   0.30   3RM + 3×1–2   0.60–0.90   0.90–1.20  pre-peaking
w4   3×3 @75%   0.36   1RM test            —     0.36       taper
```

Those land where the chart's own structure says they should, which is
corroboration this agent did not arrange.

**INOL is not load-bearing for this rule, and that is worth saying plainly.**
Everything the rule needs is structural or unanimous: SBS's trough is week 4
under INOL, under top-set intensity, and under the plain fact that it is the
taper-and-test week. INOL earns its place *describing* the fit — the operator's
4-3-2-1 against a 2-3-4-1 being an inverse — rather than deciding it. It would
become load-bearing for a strength programme whose deload is not announced by
its structure. SBS is not one.

**Neither test can be scored by the metric it establishes.** The FTP test names
no zone, so it scores zero TSS. The 1RM test is one repetition at 100%, so INOL
is `1/(100 − 100)` and is undefined. Both are the day that measures the number
every other prescription in that programme is a share of, and both are excluded
rather than scored — for opposite arithmetic reasons.

## Both disciplines open with a test microcycle

This agent laid the autumn out as thirteen gym weeks against twelve cycling
microcycles and spent two exchanges arguing about where to put the spare week.
The operator:

> "you're forgetting the entry test, the point behind a programme provider
> provides it's own standalone test microcycle."

**There is no spare week.** 0016 already makes a `Test` a programme in its own
right, and `autumn-entry-test` is the gym provider's. The cycling provider has
one too — the FTP warm-up and the twenty-minute test, which
`peloton::mapping::session` has returned as a pair of classes since before any
of this. So:

```text
              week 1          weeks 2-13
gym       entry test      3 SBS cycles of 4
cycling   FTP test        3 mesocycles of 4
```

Thirteen against thirteen, opening the same day, and the structure is symmetric
rather than reconciled.

## What the rule decides

Checked against the autumn as `docs/roadmap.md` schedules it, from w/c
2026-09-14.

**It does not choose the cycling pairing.** `test + Base(8) + Build(4)` and
`test + Build(4) + Peak(8)` both pass, and every gym test week lands on a cycling
test or deload in each. Roadmap open question 1 stays open and is now known to be
*unforced* rather than merely unanswered.

**It fixes Build at four microcycles, independently of 0032.** One test
microcycle plus eight plus four is thirteen exactly; five would give fourteen and
put every gym test week on a cycling working week. 0032 chose sessions 1+3 of
µ2–5 because that candidate diverged least in zone profile. **This reaches the
same answer from an unrelated direction** — the first corroboration either has
had.

**It makes the FTP work due rather than merely reopened.** 0033 reopened it on
finding six effect-dated values in the record. This says the block cannot start
without a fresh one: every zone in twelve weeks of prescription is a share of a
number whose most recent value, 172 from 2026-07-22, is nearly eight weeks stale
by 14 September.

**It hands open question 1 an input it did not have.** Base carries no FTP test
of its own, so the two pairings re-anchor the zones at different rates:

```text
A   test + Base(8) + Build(4)     FTP tested at weeks 1 and 13
B   test + Build(4) + Peak(8)     FTP tested at weeks 1, 5 and 13
```

Option A rides eleven weeks on one number. That is a training judgement and is
not settled here.

## What it costs

**Two agreeing criteria are corroboration, not proof.** 0032's divergence score
and this rule both put Build at four microcycles, and both are heuristics the
operator has agreed with rather than results anyone has demonstrated.

**The 5% tolerance is this agent's**, even though the separation it sits in is
the data's.

**The rule says nothing about spacing.** Whether the 1RM test and the FTP test
sit far enough apart *within* week 1 or week 13 is the allocator's question,
resolved against the operator's slots — 0032 already recorded that spacing is not
the provider's to state.

**Both tests share a week wherever the rule passes.** Weeks 1 and 13 in both
pairings, and week 5 as well in Build+Peak. That is forced: both disciplines test
in the last week of a four-week block, so the phase that satisfies the rule is
exactly the phase that co-locates the tests. It is the intended case rather than
a tolerated one — the operator's rule reads *"a test week in one should be a test
week or relative deload in the other"*, and a test week is the first answer it
names.

## Consequences

- The planner admits or refuses an arrangement rather than ranking one. Where
  several are admitted the answer is a set, which is what 0029 asked for.
- ~~`is_three_to_one` stays keyed on `hard_share` and is not rewritten.~~
  **Amended 2026-09-05, the same day.** Written on the belief that `hard_share`
  sufficed, because ranked within a run of four it agrees with TSS on Build and
  on Peak — the disagreement reported in issue #71 came of comparing a
  five-microcycle shape against a four-microcycle window. But it cannot see
  *Boost Your Base* at all, whose hard shares are eight zeros, so a rule keyed on
  it cannot find the mesocycles this decision's own calendar rests on.
  `is_three_to_one` is replaced by `cycling::shape::mesocycles`, which takes any
  sequence of scores and asks the question this decision actually states: does
  the run end at its bottom level, with something above it. Handed hard shares it
  reproduces the old answers; handed TSS it finds Base's two mesocycles.
- The cycling side needs an authored programme that can hold a `Test` microcycle
  ahead of its periodisations, the way the gym's already does.
