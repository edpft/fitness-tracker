# 0025 — A cycling session is duration × power zone

**Date**: 2026-09-01

**Implements** the cycling half of `0024-a-published-programme-is-transcribed-not-derived.md`.
**Extends** `0022-a-reference-names-a-place-at-the-destination.md` to a second
destination, and `0017-a-destination-is-a-renderer-that-returns-a-receipt.md`
with a destination that also serves as a source.

The programme data this rests on is `docs/cycling-peak-your-power-zones.md` —
all twenty-five classes of Peloton's *Peak Your Power Zones*, transcribed and
matched to sessions by the operator.

## Decision

**1. A cycling session is an ordered sequence of `(power zone, duration)`, with a
warm-up and a cool-down either side.** Settled by the operator, 2026-09-01:
*"otherwise we're importing Peloton into our domain model."* A Peloton class is a
thing at a destination that **realises** a session; its `classId` is a reference
at the adapter, exactly as 0022 has it for Hevy, and is not the session's
identity.

**2. Power zones run 1 to 7.** All seven are observed. An earlier reading of the
data said "1 to 5", was corrected to "at least 1–6" when Z6 appeared, and
corrected again when Z7 appeared at week 7 day 1. Sizing the type to the
evidence would have been wrong at both intermediate moments.

**3. The ride may be empty, and that is the test.** The FTP test prescribes
`Cycling · 20 mins` with no intervals at all, because **a zone is a share of FTP
and this ride measures FTP** — prescribing it in zones would be circular.

This is structurally identical to the gym's `WorkUp` (decision 0023): a
repetition count and no load, because the load is discovered.

```
gym       WorkUp   reps,     no load   →  discovers the load
cycling   test     duration, no zone   →  discovers the output
```

Both sit at the end of their programme and both measure the number every other
prescription in that programme is a share of. **Neither was designed for the
other**; the gym variant was written a week earlier for unrelated reasons.

**4. Cadence is not modelled.** Settled by the operator. The only cadence in the
programme is the warm-up spin-ups — *"2-3 x spin up 20-40 seconds @ 120RPM"* — and
no working interval anywhere prescribes an RPM. A cadence axis serving one
section the model does not otherwise carry would be a type built for a single
case.

**5. The warm-up and cool-down are durations, not interval sequences.** The
operator's description of every warm-up in the programme:

> a couple of minutes @ Z1, 2-3 x spin up 20-40 seconds @ 120RPM, a build
> touching each of the zones the ride touches, a final minute or so in Z1.

**It is a function of the ride**, and it checks out: read as `Z1 + 2 spin-ups +
a build from Z2 to the ride's peak zone + Z1`, it predicts the app's own
warm-up movement counts to within one across all twenty-three classes that have
one. So nothing is lost by carrying it as a duration.

Schedule **~10 minutes of warm-up**. The cool-down is **Peloton's ~1 minute plus
the operator's own 5-minute ride**, so about 6 minutes — session length is the
class length plus five.

## The days: 1 and 6

The operator takes two of the programme's three sessions. Days 1 and 6, on two
independent grounds that were not arranged to agree.

**It preserves the programme's shape.** As proportions of timed ride, summed
absolute divergence from the full programme across all seven zones:

| pairing | divergence | note |
|---|---|---|
| days 1+3 | 33.2 points | exaggerates every week; 60% hard in week 5 against 35% |
| **days 1+6** | **7.6 points** | worst zone Z4 at 11.9% against 15.1% |
| days 3+6 | 20.5 points | erases week 2's hard work; never reaches Z7 |

**And it takes the FTP retest**, which sits on week 8 day 6. Days 1+3 would run
the whole eight weeks and never measure whether it worked — the same failure as
skipping week 4 day 2 of an SBS block. The retest observation came from the week
table days before any of the above was computed.

Two costs, recorded: about 42 minutes less Z4 across eight weeks than days 1+3,
and **week 4 day 6 shows `Unavailable`**, so one long ride of the eight needs a
substitute chosen by zone profile.

## The calendar was already settled, and is in the store

**Authored 2026-08-25, in `training_slot`** — not decided here, and the operator
had to point out it already existed:

| slot | discipline | role |
|---|---|---|
| Monday evening | gym | light |
| Wednesday evening | cycling | — |
| Friday evening | gym | **heavy** |
| Sunday morning | cycling | — |

His constraints, in his words: *"the longest cycling session has to take Sunday
morning, the heaviest gym workout has to take the Friday evening, so that there's
nothing before it, the sessions have to alternate."*

Two mappings follow with nothing left to choose:

- **Sunday morning is programme day 6**, because day 6 is the long ride.
  Wednesday evening is day 1.
- **Friday is SBS day 2** — the repetition-maximum day, and in week 4 the 1RM
  test — because `programme_weekday` already records Friday as `heavy`. Monday is
  SBS day 1.

**A session asking the operator for this would be the wizard's mistake again**
(decisions 0019 and 0020): a question is worth asking only if he is the one who
knows the answer, and here the store knew.

## Peloton is a source and a sink

Settled by the operator: *"yes, Peloton should be a source as well as a sink.
ideally, we would use Peloton's scheduling functionality to add a workout to the
Peloton calendar."*

So the earlier framing — *"cycling prescription can degrade to a calendar entry
with a link to the class"* — is the **fallback**, not the target. The target is a
real destination that schedules the class in his Peloton calendar and returns a
reference, which is 0017's renderer-with-a-receipt and 0022's reference-names-a-
place, unchanged.

Being a source as well is what keeps § 11 honest: without it a cycling
prescription could never be compared against a performance, and the discipline
would have expectation with no reality beside it.

## FTP: the anchor, and a number that is not yet confirmed

The operator's most recent test, **2026-07-22**, from the app's own summary:
20 minutes, **average output 181 watts**, best 291, average cadence 90 rpm,
average heart rate 162.

He notes: *"I think the value that Peloton takes is slightly lower than the
average watts. I don't think they expose it in the app."*

**The published convention is 95% of the twenty-minute average**, which gives
**172 W**. That matches "slightly lower" and is the standard derivation, but it
is a convention applied here rather than a number read off his account, so it is
**recorded as unconfirmed**.

It is confirmable in one glance without Peloton exposing FTP directly, because
the zone boundaries are shares of it. At 172 W:

| zone | % of FTP | watts |
|---|---|---|
| Z1 | < 55% | < 95 |
| Z2 | 56–75% | 96–129 |
| Z3 | 76–90% | 131–155 |
| Z4 | 91–105% | 157–181 |
| Z5 | 106–120% | 182–206 |
| Z6 | 121–150% | 208–258 |
| Z7 | > 150% | > 258 |

If the app's power zone ranges match these, FTP is 172. If they do not, FTP is
whatever they divide back to, and this table is wrong rather than the app.

**FTP is an interpretive parameter under § 13** — effect-dated and retained, never
overwritten. The value in force from 2026-07-22 is this one; the week 8 retest
supersedes it without rewriting anything derived from it. This is § 13's first
real instance, and the zones are the reason: the same class prescribes different
watts before and after a test, and a past prescription has to stay reproducible.

## Not settled

**Whether the warm-up's zone durations are ever needed.** They are unpinned —
"a couple of minutes", "20-40 seconds", "a minute or so" — and every total in the
transcription excludes them rather than estimating them. About four and a half
hours of riding across the programme. If a prescription must state the whole
session, this has to close; if the warm-up is a preparation the operator rides to
feel, it does not.

**Which class substitutes for week 4 day 6.** It must be chosen by zone profile —
`5/7/9/7/5`, 32:58 of Z3, 12:58 of Z2 — which is exactly the catalogue search
that adopting a published programme otherwise avoids.

**When the programme starts**, and how its eight weeks align to the SBS blocks.
The structures match — three build weeks and a recovery week, twice, with the
light weeks and test weeks coinciding — but no start date is chosen.
