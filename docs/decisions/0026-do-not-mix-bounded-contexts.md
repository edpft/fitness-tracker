# 0026 — Do not mix bounded contexts

**Date**: 2026-09-02

**Explains**: why `0023-a-block-derives-its-loading-and-discovers-its-result.md`
failed, and why `0024-a-published-programme-is-transcribed-not-derived.md`
worked. Neither is amended; this names the cause they share.

**Constrains**:
`0018-a-programme-counts-cycles-and-the-scheduler-owns-the-calendar.md`. 0018
said a programme counts cycles and never learns a date. This says the counting
is *all* that crosses between programmes, which is a stronger claim than 0018
made and the reason it is the right shape.

## Context

The operator's rule, in his words, on 2026-09-02:

> "I think what we learnt from the aborted block periodisation and the recent
> SBS branches is: don't mix bounded contexts. we tried to fit my CrossFit gyms
> programming into classically block periodisation and it didn't work, we
> noticed that rep maxes had a different meaning in SBS, we're about to try and
> fit Peloton's power zone programming into Friel's programming and the answer
> is: don't, these are two different things."

Three failures with one cause, and the third had not happened yet.

**The gym block.** 0023 took the accumulation and intensification phase names
from the operator's old CrossFit gym and fitted them to a periodisation model
from a different tradition, then invented "realisation" to close the gap. The
output was `8 × 2` prescribed to stay inside Prilepin's bands. The names came
from one context, the loading model from another, and the third phase from
nowhere at all.

**The rep max.** SBS's `1×8 @ 8RM` looks like the rep max used everywhere else
and is not. It is an input to a table SBS publishes, which converts it to a
training maximum for the following week. Research into standard rep-max
protocols on 2026-09-02 found the established ones treat a multiple-RM as a
*predictor of a 1RM*, and warn that going above five repetitions degrades that
prediction. The warning is real and aimed somewhere else: SBS predicts nothing.
Adopting the warning would have been importing a foreign meaning of the same
words.

**The near miss.** The operator's own programming vocabulary comes from Joe
Friel's *The Triathlete's Training Bible*, and the long-term intention is to
pick Peloton classes that satisfy Friel-style prescriptions. An agent proposed
labelling the transcribed *Peak Your Power Zones* sessions with Friel's six
abilities. Checked against the transcription before it was built, the mapping
fails in a way that is structural rather than incidental:

- Three of Friel's six abilities have no representation in the programme at
  all. **Muscular force** is torque — low cadence, big gear — and PYPZ
  prescribes watts and never a gear. **Speed skills** is high-cadence
  efficiency, and `docs/cycling-peak-your-power-zones.md` records that the only
  cadence anywhere in the programme is the warm-up spin-ups.
- Friel's advanced abilities are *combinations* of the basic ones — muscular
  endurance sits between endurance and force. Labelling a threshold ride
  "muscular endurance" claims a force component the session cannot prescribe.

The two taxonomies do not share an axis. Friel distinguishes abilities by **how
the watts are made**; a power zone is defined solely by **output** and is
deliberately agnostic to cadence. No care in the transcription produces a force
session from a power-zone class.

## Decision

**1. Each published programme is its own bounded context and keeps its own
vocabulary.** SBS has training maxima and rep-max days. *Peak Your Power Zones*
has power zones and Peloton's own class naming — Endurance Ride, Power Zone
Ride, Max Ride. Friel, if he is ever built, has abilities and periods.

**2. Nothing translates between them.** There is no shared training model in
`domain` of which the three are special cases. That model is what 0023 tried to
build, and building it is the failure this decision names.

**3. What crosses is counting, not training.** A programme declares:

```text
microcycles                    how long it is
sessions per microcycle        how much it wants of you
microcycles per mesocycle      where its unloading falls
```

None of those is a training concept. None of them exports a watt, a kilogram,
a zone or a repetition.

**4. The session count is an input to the programme, not a filter outside it.**
The operator, 2026-09-02:

> "we need some way to encode how we want 2 of the 3 sessions that Peloton's
> Peak Your Power Zones programme offers, so it can't just be given me the next
> session in this microcycle, it needs to be I want 2 sessions per microcycle."

So PYPZ is told it has two sessions per microcycle and answers with days 1 and
6. *Which* two best preserve the programme is knowledge inside PYPZ's context —
0025 established that 1+6 preserves the zone distribution four times better than
1+3 and is the only pairing taking the week 8 retest — and that reasoning must
not be re-derived by a planner that does not know what a zone is. SBS asked for
two answers with both of its sessions, and would refuse one.

**5. Two programmes are compatible when they agree on microcycles per
mesocycle.** The operator, 2026-09-02:

> "you could just ask programmes to declare how many microcycles per mesocycle,
> the thing that makes SBS and Peloton work together is that the first is
> explicitly 1x 4 weeks and the other is implicitly 2x 4 weeks."

This is alignment **by construction rather than by rule**. A mesocycle boundary
*is* the unloading week, so two programmes with four-microcycle mesocycles have
their unloading weeks in the same place without either knowing anything about
the other.

### The evidence for PYPZ being 2 × 4

The programme states eight weeks and no internal division. The division is
visible in the transcription:

| mesocycle | loading | unloading |
|---|---|---|
| 1 | week 1 base, week 2 threshold, week 3 VO2 | **week 4** deload — `F`, Z3 23:00, *less* aerobic work than the base week |
| 2 | week 5 VO2 sustained, week 6 anaerobic Z6, week 7 Max Ride Z7 and the 90-minute ride | **week 8** taper and FTP retest |

And the closing sessions of the two mesocycles are the same session twice.
`docs/cycling-peak-your-power-zones.md` recorded this under *The taper and the
deload are the same session twice* as a curiosity about instructors: `F` (week
4 day 1) and `V` (week 8 day 1) are both Christine D'Ercole, both pyramids,
23:00 and 23:01 of Z3 — one second apart. Read as a mesocycle boundary it stops
being a curiosity and becomes the structure showing itself.

SBS is the same 3:1 shape: weeks 1–3 load, and week 4 is the chart's lightest
percentage followed by the test.

## Rejected

**A microcycle declaring its load and whether it measures.** Proposed by an
agent on 2026-09-02, so that a planner could enforce "do not put a measurement
on another programme's heavy week". The argument for it was that fatigue is the
one thing two contexts genuinely share, being imposed on one body.

The argument is not wrong and the mechanism is still unnecessary. Counting
microcycles per mesocycle gets the same alignment with strictly less: no shared
load vocabulary, no rule to apply, nothing to leak. A relative-load declaration
is a training concept re-entering by the back door, and the thing it buys was
already free.

## Consequences

- **`Selection` goes.** `domain::cycling::programme::Selection` maps
  `Weekday → CycleDay`, which is a programme knowing about days, and
  `cli::cycling::selection()` hardcodes Wednesday and Sunday into it. What
  replaces both is a count in and sessions out.
- **The gym/cycling split is the session counts.** It was listed as a planning
  input with nowhere to live. SBS at two per microcycle plus PYPZ at two per
  microcycle is four sessions against the four slots the schedule holds.
- **A cycling session stays in power zones.** The "intention" the operator asked
  for on 2026-09-02 is Peloton's own three-valued class naming, not an
  adaptation model — published naming rather than anything read off the zone
  profiles.
- **Friel is a future context, not a translation target.** Building it means
  transcribing a text and prescribing in its terms, the same way SBS was. It
  does not mean labelling anything that already exists.
- **The planner tiles mesocycles.** Which is why the operator's answer to the
  span question was "as many 4 week blocks as we can, plus a 1 week entry test
  block" — the four is not arbitrary, it is the mesocycle both programmes share.

## Open

**What the one-week entry test is, in this counting.** A mesocycle of one
microcycle is the obvious answer and is suspicious for exactly that reason: it
makes a new case a degenerate version of an existing type, which is the shape
mistake this project has made before. Not settled here.
