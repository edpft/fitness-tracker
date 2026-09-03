# 0026 — Do not mix bounded contexts

**Date**: 2026-09-02

**Amended**: 2026-09-03. Decision 5 narrowed to the special case it is, the
rejected alternative reinstated, and decision 4 generalised from sessions to
microcycles. The assumption decision 5 rested on met its counterexample the
following day. See *Amendment, 2026-09-03* at the end; the amendment governs
where it and the original disagree.

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

**The gym block.** 0023 took the operator's old CrossFit gym's programming and
fitted it to classical block periodisation. The output was `8 × 2` prescribed to
stay inside Prilepin's bands.

**"Realisation" was not an agent's invention, and the correction matters.** The
operator, 2026-09-02:

> "I saw that block periodisation programmes tended to be broken down into
> accumulation, transmutation, and realisation blocks and told the agent that we
> needed a realisation phase. the thing we both missed was that the CrossFit
> gym's programme just wasn't classical block periodisation and try to make it
> fit created something else."

The vocabulary was correct — accumulation, transmutation and realisation is the
standard division — and it was correctly recalled. What was wrong was applying
it to a programme that was not of that type. So the failure is not a bad name or
a missing source; it is a true structure imposed on material that did not have
it, which is a harder mistake to see and the reason this decision exists.

**The two lessons, in his words**: *"don't force one type of programming to fit
the structure of another type of programming and take differences of vocabulary
seriously."*

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
has power zones, Peloton's own class naming, and an intent per session that is
not reducible to either. Friel, if he is ever built, has abilities and periods.

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

Two programmes with four-microcycle mesocycles then have their unloading weeks
in the same place without either knowing anything about the other.

**This rests on an assumption, and the operator named it rather than letting it
pass**, 2026-09-02:

> "this is probably true but it is an assumption, what you were suggesting was
> probably more likely to be correct because it made the load management
> explicit however it was much more complicated. my proposal is much simpler
> but, as I say, it rests on an assumption that a programme (or block in a
> programme) will build to a test (or some other deload)."

So this is a decision taken on simplicity with a known risk, not a proof. The
assumption is that **a mesocycle builds to a test or some other unloading**, and
it holds for both programmes on the table — SBS's week 4 and PYPZ's weeks 4 and
8. A programme that unloads mid-mesocycle, or does not unload at all, breaks it
silently: the arrangement would look aligned and would not be.

**What to do if it breaks**: adopt the rejected alternative below, which makes
load management explicit and does not depend on where a mesocycle ends. It was
rejected for cost, not for correctness.

**It broke on 2026-09-03.** Peloton's *Build Your Power Zones* is five
microcycles. See the amendment.

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

**Rejected for cost, not for correctness**, and the operator was explicit that
it was *"probably more likely to be correct because it made the load management
explicit"*. Counting microcycles per mesocycle reaches the same alignment for
the two programmes on the table with far less machinery — no shared load
vocabulary, no rule to apply, nothing to leak — at the price of the assumption
recorded under decision 5.

This is therefore a live alternative rather than a closed one. It is what
decision 5 falls back to if a programme turns up that does not build to its
unloading, and nothing here should be read as an argument that a shared
load vocabulary would be wrong in principle.

**Reinstated 2026-09-03.** That programme turned up the next day.

## Consequences

- **`Selection` goes.** `domain::cycling::programme::Selection` maps
  `Weekday → CycleDay`, which is a programme knowing about days, and
  `cli::cycling::selection()` hardcodes Wednesday and Sunday into it. What
  replaces both is a count in and sessions out.
- **The gym/cycling split is the session counts.** It was listed as a planning
  input with nowhere to live. SBS at two per microcycle plus PYPZ at two per
  microcycle is four sessions against the four slots the schedule holds.
- **A cycling session stays in power zones — and still needs an intent.** The
  operator, 2026-09-02: *"two Power Zone Rides with the same amount of Z5 can be
  different if they are structured differently to elicit different adaptation.
  the mistake was to try and use Friel's vocabulary to describe those
  intentions."*

  So the requirement he stated on 2026-09-02 survives this decision intact: a
  session is not fully described by its duration and its time in each zone.
  What this decision rules out is *where the words for it come from*, and it
  rules out two sources rather than one — Friel's abilities, which belong to
  another context, and the archetypes an earlier session read off the zone
  profiles, which describe the transcribed sample rather than name a vocabulary.
  Peloton's own class naming is three-valued and does not separate week 3's
  thirty-second bursts from week 5's sustained blocks. **Where the intent
  vocabulary does come from is open**, and it is the operator's to settle.
- **Friel is a future context, not a translation target.** Building it means
  transcribing a text and prescribing in its terms, the same way SBS was. It
  does not mean labelling anything that already exists.
- **The planner tiles mesocycles.** Which is why the operator's answer to the
  span question was "as many 4 week blocks as we can, plus a 1 week entry test
  block" — the four is not arbitrary, it is the mesocycle both programmes share.

## Open

**1. Where the entry test lives.** The operator framed the problem on
2026-09-02: *"neither SBS nor PYPZ plan an entry test but both require an input
that can only really come from a test, be that a 1RM or an FTP test."* Both
programmes exit on a test and neither opens on one, so the first run of either
in a season has nothing to be a share of.

Two independent choices, and he named both:

| | |
|---|---|
| **where it lives** | a standalone programme — or a part of the programme itself, needed only the first time that programme is run in a season |
| **where its content comes from** | the testing week each programme already has — or one we invent |

An earlier draft of this decision recorded a different worry here: that a
one-microcycle mesocycle would make a new case a degenerate version of an
existing type. That worry is misplaced. Decision 0013 already makes a test a
programme in its own right — *"one week, no ladder"* — and a test genuinely is
one microcycle, so the standalone option names a category that exists rather
than subtracting a feature from one that does not fit.

**2. Where a cycling session's intent vocabulary comes from.** See the
consequence above. This decision rules out Friel's abilities and rules out
naming archetypes from the transcribed sample; it does not say what replaces
them.

---

## Amendment, 2026-09-03

### What happened

Decision 5 said two programmes are compatible when they agree on microcycles
per mesocycle, and recorded that this assumes a mesocycle builds to a test or
some other unloading. The operator flagged it as an assumption when he proposed
it: *"what you were suggesting was probably more likely to be correct because it
made the load management explicit however it was much more complicated."*

It held for a day. Peloton publishes four power-zone programmes, and reading all
four found no four-week one at all:

| programme | microcycles | sessions/microcycle | shape | FTP test |
|---|---|---|---|---|
| Discover Your Power Zones | 5 | 4–6 | onboarding | weeks 1 **and** 5 |
| Boost Your Base | 8 | 3 | unloads at 4 and 8, all Z2–Z3 | **none** |
| Build Your Power Zones | 5 | 3 | 4 loading, then recovery | week 5 |
| Peak Your Power Zones | 8 | 3 | deload at 4, taper at 8 | week 8 |

**Build is five microcycles**, four loading and a recovery week. Run beside
three SBS cycles across a fourteen-microcycle span, two unloading weeks
coincide and the third does not: the term's heaviest single lands in the same
week as Build's hardest ride, and the following week the bike tests while the
gym has nothing. Counting cannot see that, because four and five do not divide.

The operator, on being shown it:

> "congratulations, you've rediscovered one of the main reasons for this tool to
> exist! If every published training programme was a 4 week cycle ending in a
> deload or a test, I wouldn't need this!"

**That is the correction.** Decision 5 solved the case where the problem does
not arise and called it settled. Misalignment between published programmes is
not an edge case the planner tolerates; it is the thing the planner is for.

### What the objective actually is

Not "test weeks land on deloads", and not "mesocycle lengths agree". Those are
proxies that happened to hold for two programmes that were both fours. The
operator named the real one, 2026-09-03:

> "we want to be trying to ensure that the fatigue profiles coincide"

Fatigue is what two programmes genuinely share, being imposed on one body. That
was the argument for the rejected alternative and it was right; what was wrong
was accepting a cheaper proxy for it.

### Decision 4 generalises from sessions to microcycles

The operator, 2026-09-03:

> "what we could do is take the approach we took with taking 2 sessions a
> mesocycle from PYPZs programmed 3 sessions a mesocycle and ask, say, build to
> provide a 4 microcycle mesocycle that confirms as closely as possible to the
> written programme."

So a programme is asked for a **shape** — so many sessions per microcycle, so
many microcycles — and answers in its own terms. Which of PYPZ's three sessions
to take, and what Build drops to fit four, are both knowledge inside those
programmes' contexts. The planner never learns what a power zone is.

### And the load declaration sits underneath it, not against it

**"Give me four microcycles" has more than one honest answer**, and they are not
equivalent for fatigue:

- Build 1–4 conforms most closely to what is written and ends on *Peak
  Intensity* — a mesocycle finishing at its hardest, the opposite of SBS's
  week 4.
- Build 1, 2, 3, 5 conforms less closely and ends on recovery, so the profile
  matches.

Build can offer either, honestly, in its own terms. What it cannot do is know
which is wanted, because that depends on what the gym is doing that week and it
is not allowed to know. So something outside has to compare them, and comparing
fatigue profiles means each microcycle saying how heavy it is relative to its
own programme's range, and whether it measures.

**The two mechanisms compose**: the programme offers the shapes it can honestly
take, and the planner picks the one whose fatigue profile coincides. Neither
replaces the other, which is why rejecting the declaration in favour of counting
was a false economy rather than a simplification.

### Decision 5, as it now stands

Agreeing on microcycles per mesocycle **aligns unloading weeks where it holds,
and is worth checking first because it is free**. It is not the definition of
compatibility and it is not sufficient. Compatibility is coincident fatigue
profiles, established by comparison, and available whether or not the lengths
divide.

### Also found, and it confirms 0027

**Boost Your Base contains no FTP test and says to test before starting.** A
published programme stating an input it cannot itself produce is exactly the
shape 0027 gives every programme, and the operator's rule that a test's position
is derived rather than placed. It is corroboration from outside this project,
which is the only kind this project's documents can get.

## Open, after the amendment

**Who decides what a programme drops when asked to conform.** Build answering
"four microcycles" is a judgement made by whoever transcribes Build, reading
Build. That is the right place for it under this decision, but it is a decision
to record at transcription time rather than something to let fall out of an
implementation. The judgement relocates; it does not disappear.
