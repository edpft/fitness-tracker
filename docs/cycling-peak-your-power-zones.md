# Peak Your Power Zones — transcribed

**Source**: operator screenshots of the Peloton app, 2026-09-01, 17:56–18:04.
This is **data**, transcribed from what the app displayed. Not analysis, not a
specification.

## Why the class plans are transcribed at all

**The domain session is duration × power zone. Peloton is an adapter.**

Settled by the operator, 2026-09-01: *"the idea behind transcribing all the class
plans was to analyse what these classes are doing in terms of duration and
intensity (specified duration in each power zone) because, otherwise we're
importing Peloton into our domain model."*

So a prescribed cycling session is an ordered sequence of `(zone, duration)` and
nothing else — the same kind of thing a prescribed gym session is, stated in the
domain's own terms. A **Peloton class is a thing at a destination that realises
one**, and the link that names it is a reference at the adapter, exactly as
decision 0022 has it for Hevy. It is not the session's identity.

An earlier revision of this document had that backwards. It reported "only the
class link identifies a class" as *the finding that shapes the design*, which
would have made the prescription a Peloton class id with a domain wrapper — a
source's format shaping the domain, which § II.3 rules out in as many words.
The link is still needed, and still the only unambiguous name a class has; it
is just not what a cycling session *is*.

**Choosing a published programme is a convenience about matching, not about
modelling.** The operator: *"choosing a published Peloton programme just makes
matching the prescribed domain sessions easier as we don't have to find them."*
Somebody has already assembled a coherent eight-week sequence and each session
already has a class that realises it. Without that, every prescribed session
would need a search of the catalogue for a class whose zone profile fits.

**And it makes the day-pairing a computable question rather than an opinion.**
The operator again: *"it also allowed you to notice that picking days 1 and 3,
instead of days 1 and 6 or 3 and 6 makes this a different programme."* Once every
class is transcribed and matched, the three pairings are three different
distributions of time across zones, over eight weeks, and the difference can be
computed instead of argued about.

## The programme

From the overview screen, verbatim:

| | |
|---|---|
| classes | 25 |
| weeks | 8 |
| frequency | 3× per week |
| class length | 10–90 minutes |
| instructors | Olivia Amato, + 4 more |

> Welcome to Peak Your Power Zones, an advanced program designed to improve your
> FTP in 8 weeks. Before starting, make sure you have installed the Power Bar on
> your touchscreen and have taken the FTP Test. Are you ready?

**The week is a fixed seven-day pattern**, identical in all eight weeks:

```
Day 1  class          Day 5  recovery
Day 2  recovery       Day 6  class
Day 3  class          Day 7  recovery
Day 4  recovery
```

Seven weeks of three classes plus week 8's four is 25, reconciling exactly.

## The eight weeks, complete

| week | day 1 | day 3 | day 6 |
|---|---|---|---|
| 1 | 45 Endurance — Wilpers | 45 Endurance — Amato | 60 Endurance — Alldis |
| 2 | 45 Power Zone — Morton | *(obscured)* — Amato | 60 Endurance — D'Ercole |
| 3 | 45 Power Zone — Alldis | 45 Power Zone — Amato | 60 Endurance — Morton |
| 4 | 45 Endurance — D'Ercole | *(obscured)* — Wilpers | 60 Endurance — Morton |
| 5 | 45 Power Zone — Wilpers | 45 Power Zone — Morton | 60 Endurance — Alldis |
| 6 | 45 Power Zone — D'Ercole | 45 Power Zone — Wilpers | 60 **Power Zone** — D'Ercole |
| 7 | 45 Power Zone **Max** — Alldis | 45 Power Zone — D'Ercole | 90 Endurance — Wilpers |
| 8 | 45 Endurance — D'Ercole | *(obscured)* — Morton | **10 min FTP Warm Up + FTP test** — Wilpers |

Three day-3 titles were hidden behind the `Join program` button; the instructor
is legible in each. Everything else was read directly.

**The arc**: week 1 all endurance; weeks 2, 3, 5, 6 build; week 4 drops day 1
back to endurance as a deload; week 7 peaks on the only `Max Ride` and the only
90-minute ride; week 8 tapers and tests.

## Identifying a class, and what must not be stored with it

A class is named by its **`classId`** — 32 hex characters, from the app's share
link. Title and instructor do not identify one: week 6 day 1 and week 7 day 3 are
both `45 min Power Zone Ride` by Christine D'Ercole, and two different `60 min
Power Zone Endurance Ride` classes share the same 13/46/1 profile.

The class detail page also carries an **original air date**, which together with
title and instructor is probably unique and is worth recording as corroboration.

**Strip everything else off the share link.** A link as the app produces it looks
like:

```
…/classes/cycling?modal=classDetailsModal&classId=<32 hex>&code=<base64>&utm_source=…
```

The `code` parameter is base64 of two further 32-hex ids joined by `|`, neither of
which is the class. It is a share/attribution token and one of the two is
plausibly the operator's own Peloton user id, so **it is not committed** (§ 35 in
spirit: a personal identifier is not repository content). `utm_*` is analytics
and equally not content. **`classId` is the whole of what identifies the class.**

## Matched classes

Supplied by the operator, one at a time. This is the authoritative mapping; do
not extend it by inference.

| week | day | key | title | instructor | aired | `classId` |
|---|---|---|---|---|---|---|
| 1 | 1 | `M` | 45 min Power Zone Endurance Ride | Matt Wilpers | 2022-05-13 15:00 | `7a077ff36228426794bd3adc362ca757` |

`M`'s intervals and its 24:00 of Z3 against 9:00 of Z2 are below, and it lands
where the week table says week 1 day 1 should be: an endurance ride by Wilpers.

## Time in zone, per transcribed class

The analytical unit. Ride portion only — the app gives no zone breakdown for the
warm-up or cool-down, so those are excluded and named separately.

| key | kind | WU | Z1 | Z2 | Z3 | Z4 | Z5 | ride |
|---|---|---|---|---|---|---|---|---|
| B | 45 Power Zone | 11 | 6:00 | — | 10:00 | **17:00** | — | 33:00 |
| J | 45 Power Zone | 11 | 3:00 | — | 12:00 | **18:00** | — | 33:00 |
| H | 45 Power Zone | 12 | 6:00 | — | 9:59 | **15:59** | — | 31:58 |
| D | 45 Power Zone | 12 | 10:00 | — | — | **15:00** | 7:00 | 32:00 |
| I | 45 Power Zone | 12 | 8:00 | 4:00 | — | **15:00** | 5:00 | 32:00 |
| E | 45 Endurance | 10 | — | 8:00 | 26:00 | — | — | 34:00 |
| L | 45 Endurance | 10 | — | 6:01 | 27:59 | — | — | 34:00 |
| M | 45 Endurance | 11 | — | 9:00 | 24:00 | — | — | 33:00 |
| F | 45 Endurance | 12 | — | 9:00 | 23:00 | — | — | 32:00 |
| A | 60 Endurance | 11 | — | 11:00 | 37:00 | — | — | 48:00 |
| G | 60 Endurance | 12 | — | 10:01 | 36:59 | — | — | 47:00 |
| C | 60 Endurance | 13 | — | 12:58 | 32:58 | — | — | 45:56 |
| K | 60 Endurance | 13 | — | 16:00 | 30:00 | — | — | 46:00 |

**Three archetypes fall out, and one number holds across two of them.**

- **Endurance** — Z3 with Z2 float, no Z4 at all. The 45s run 23–28 minutes of
  Z3; the 60s run 30–37.
- **Threshold** (`B`, `J`, `H`) — 16–18 minutes of Z4, the balance in Z3, Z1 to
  recover. No Z5.
- **VO2** (`D`, `I`) — 15 minutes of Z4 *and* 5–7 of Z5, with much more Z1
  because the recoveries have to be real.

**Every 45-minute Power Zone ride holds Z4 at 15–18 minutes.** What the programme
varies is what sits beside it: Z3 early, Z5 late. That is the progression, and it
is visible in the numbers rather than asserted over them.

`I` is the only class using 30/30s — five Z5 efforts with four Z2 recoveries,
twice.

## Transcribed classes — the intervals

Thirteen of twenty-five, **none yet matched to a week or day**; the operator is
matching them explicitly. Keys are arbitrary. Every sequence was checked against
the app's own `Cycling` total and all thirteen reconcile.

Durations are as displayed and **not round** — `4:56`, `7:02`, `1:59`, `8:01`.
These are recorded classes and the intervals fall where the instructor put them,
so **a prescribed cycling interval carries seconds, not minutes.**

### `45 min Power Zone Ride`

| key | intervals |
|---|---|
| B | `Z4 3:00 · Z3 3:00 · Z4 2:00 · Z1 3:00 · Z4 4:00 · Z3 4:00 · Z4 3:00 · Z1 3:00 · Z4 3:00 · Z3 3:00 · Z4 2:00` |
| J | `Z4 4:00 · Z3 3:00 · Z4 3:00 · Z3 3:00 · Z4 2:00 · Z1 3:00 · Z4 4:00 · Z3 3:00 · Z4 3:00 · Z3 3:00 · Z4 2:00` |
| H | `Z4 2:00 · Z3 3:59 · Z4 2:00 · Z1 3:00 · Z4 4:00 · Z3 2:00 · Z4 4:00 · Z1 3:00 · Z4 1:59 · Z3 4:00 · Z4 2:00` |
| D | `Z4 5:00 · Z1 2:00 · Z5 2:00 · Z1 2:00 · Z4 5:00 · Z1 2:00 · Z5 3:00 · Z1 2:00 · Z4 5:00 · Z1 2:00 · Z5 2:00` |
| I | `Z4 5:00 · Z1 2:00 · ⟨Z5 0:30 · Z2 0:30⟩×4 · Z5 0:30 · Z1 2:00 · Z4 5:00 · Z1 2:00 · ⟨Z5 0:30 · Z2 0:30⟩×4 · Z5 0:30 · Z1 2:00 · Z4 5:00` |

### `45 min Power Zone Endurance Ride`

| key | intervals |
|---|---|
| E | `Z3 8:00 · Z2 3:00 · Z3 5:00 · Z2 2:00 · Z3 8:00 · Z2 3:00 · Z3 5:00` |
| L | `Z3 5:59 · Z2 2:00 · Z3 7:59 · Z2 2:00 · Z3 8:01 · Z2 2:01 · Z3 6:00` |
| M | `Z3 5:00 · Z2 3:00 · Z3 7:00 · Z2 3:00 · Z3 7:00 · Z2 3:00 · Z3 5:00` |
| F | `Z3 3:00 · Z2 2:00 · Z3 5:01 · Z2 2:00 · Z3 7:00 · Z2 3:00 · Z3 5:00 · Z2 2:00 · Z3 2:59` |

### `60 min Power Zone Endurance Ride`

| key | intervals |
|---|---|
| A | `Z3 9:00 · Z2 3:00 · Z3 7:00 · Z2 3:00 · Z3 5:00 · Z2 2:00 · Z3 7:00 · Z2 3:00 · Z3 9:00` |
| G | `Z3 9:00 · Z2 3:00 · Z3 9:00 · Z2 3:00 · Z3 7:00 · Z2 2:02 · Z3 6:58 · Z2 1:59 · Z3 5:01` |
| C | `Z3 4:56 · Z2 3:00 · Z3 7:02 · Z2 2:58 · Z3 9:00 · Z2 4:00 · Z3 7:00 · Z2 3:00 · Z3 5:00` — `Unavailable` |
| K | `Z3 7:00 · Z2 4:00 · Z3 7:00 · Z2 4:00 · Z3 5:00 · Z2 3:00 · Z3 5:00 · Z2 3:00 · Z3 3:00 · Z2 2:00 · Z3 3:00` |

**Not transcribed**: `45 min Power Zone Max Ride` (w7 d1), `60 min Power Zone
Ride` (w6 d6), `90 min Power Zone Endurance Ride` (w7 d6), `10 min FTP Warm Up
Ride`, the FTP test, and seven others.

### Do not reconstruct the mapping from upload order

It was tried and it fails. Reversing the upload order and threading the
timestamps gives `M, L, K, J, I, H, …`, which matches the programme from week 1
for five classes and then breaks: `H` is a 45-minute Power Zone ride where week 2
day 6 wants a 60-minute endurance ride. The sequence is right but **not
contiguous**, because not every class was captured, and nothing in a screenshot
says where the gaps fall. A wrong guess here is silent — a plausible ride that is
simply not the one the programme asks for.

## The FTP retest is on day 6

The operator takes two sessions a week. **Week 8's FTP retest is on day 6**, so
days 1 and 3 for eight weeks runs the whole programme and never measures whether
it worked — and never produces the input the next eight weeks would anchor on.

Same failure as skipping week 4 day 2 of the SBS squat block, for the same
reason. Cheapest fix is taking day 6 instead of day 3 in week 8: one session in
sixteen. A prerequisite FTP test is also required before week 1.

## Where this meets the gym

**Both disciplines are anchored on a measured maximum, and both open and close on
a test.** Cycling opens on an FTP test and retests at week 8 day 6; the SBS squat
block opens on a 1RM and retests at week 4 day 2 (decision 0024). The zones in
one and the percentages in the other are shares of a number the programme itself
measures at both ends.

**Eight cycling weeks is exactly two four-week squat blocks.**

```
cycling   FTP test │ w1 w2 w3 w4 w5 w6 w7 w8 │ FTP retest
squat     1RM test │ b1 b1 b1 b1 b2 b2 b2 b2 │ 1RM at the end of each block
```

With the time-in-zone table above and the squat chart in 0024, the interaction
becomes checkable rather than hopeful: week 7 pairs the only `Max Ride` with a
threshold ride, and squat block 2's week 3 is `3×1 @ 90%` and a 3RM. Whether those
belong in the same week is a question with numbers behind it.

## Open

**The class-to-session mapping** — the operator is supplying it explicitly.

**The remaining twelve class plans**, at least for the days actually taken.

**Which two days.** Days 1 and 3, 1 and 6, or 3 and 6 — three different
programmes, and once the mapping lands, three computable ones.
