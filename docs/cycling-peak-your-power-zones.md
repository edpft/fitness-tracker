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
| 2 | 45 Power Zone — Morton | 45 **Endurance** — Amato | 60 Endurance — D'Ercole |
| 3 | 45 Power Zone — Alldis | 45 Power Zone — Amato | 60 Endurance — Morton |
| 4 | 45 Endurance — D'Ercole | 45 **Endurance** — Wilpers | 60 Endurance — Morton |
| 5 | 45 Power Zone — Wilpers | 45 Power Zone — Morton | 60 Endurance — Alldis |
| 6 | 45 Power Zone — D'Ercole | 45 Power Zone — Wilpers | 60 **Power Zone** — D'Ercole |
| 7 | 45 Power Zone **Max** — Alldis | 45 Power Zone — D'Ercole | 90 Endurance — Wilpers |
| 8 | 45 Endurance — D'Ercole | *(obscured)* — Morton | **10 min FTP Warm Up + FTP test** — Wilpers |

Three day-3 titles were hidden behind the `Join program` button. Weeks 2 and 4
are now resolved by the mapping — both are **Endurance** rides — leaving only
week 8 day 3 unknown.

**The arc**: week 1 all endurance; weeks 2, 3, 5, 6 build; week 4 drops day 1
back to endurance as a deload; week 7 peaks on the only `Max Ride` and the only
90-minute ride; week 8 tapers and tests.

## Identifying a class, and what must not be stored with it

A class is named by its **`classId`** — 32 hex characters, from the app's share
link. Title and instructor do not identify one: week 6 day 1 and week 7 day 3 are
both `45 min Power Zone Ride` by Christine D'Ercole, and two different `60 min
Power Zone Endurance Ride` classes share the same 13/46/1 profile.

The class detail page carries a date, but it is **not** an air date and identifies
nothing — see *The date is not an air date* below. What the page does give, and
what is worth capturing, is the **movement count** on the `Class plan` summary.

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

| week | day | key | title | instructor | dated | `classId` |
|---|---|---|---|---|---|---|
| 1 | 1 | `M` | 45 min Power Zone Endurance Ride | Matt Wilpers | 2022-05-13 15:00 | `7a077ff36228426794bd3adc362ca757` |
| 1 | 3 | `L` | 45 min Power Zone Endurance Ride | Olivia Amato | 2022-05-13 15:00 | `887d10592df041ce808cb483ec05687a` |
| 1 | 6 | `K` | 60 min Power Zone Endurance Ride | Ben Alldis | 2022-05-13 15:00 | `7c55c9f4335a46f2955e2a4827bffa86` |
| 2 | 1 | `J` | 45 min Power Zone Ride | Denis Morton | 2022-05-13 15:00 | **pending** — see below |
| 2 | 3 | `N` | 45 min Power Zone **Endurance** Ride | Olivia Amato | 2022-05-13 15:00 | `709a359725cf4bffb4cdedb70a6506b0` |
| 2 | 6 | `O` | 60 min Power Zone Endurance Ride | Christine D'Ercole | 2022-05-13 15:00 | `23cd9015db5947679c321e38dd0082a1` |
| 3 | 1 | `I` | 45 min Power Zone Ride | Ben Alldis | 2022-05-13 15:00 | `b54a1b4ac2924db0bb5a72cf5a540d40` |
| 3 | 3 | `H` | 45 min Power Zone Ride | Olivia Amato | 2022-05-13 15:00 | `9c2466f479684898905f9629b3cc4c83` |
| 3 | 6 | `G` | 60 min Power Zone Endurance Ride | Denis Morton | 2022-05-13 15:00 | `d55e8e879dad415d8a3f3935dd1f4b4f` |
| 4 | 1 | `F` | 45 min Power Zone Endurance Ride | Christine D'Ercole | 2022-05-13 15:00 | `0cd72d4b70c54c8e93b5f13e75fee11d` |
| 4 | 3 | `E` | 45 min Power Zone **Endurance** Ride | Matt Wilpers | 2022-05-13 15:00 | `ed1fe2a5e2344dacb2f9bd9984d9ca83` |
| 4 | 6 | `C` | 60 min Power Zone Endurance Ride | Denis Morton | 2022-05-13 15:00 | `c67ec9512f954169acd9df4c95010e49` ⚠ |
| 5 | 1 | `D` | 45 min Power Zone Ride | Matt Wilpers | 2022-05-13 15:00 | `9cae0c2dfe234c529db4da028ff4addd` |
| 5 | 3 | `B` | 45 min Power Zone Ride | Denis Morton | 2022-05-13 15:00 | `c2c9fff7966e4743b162b5cc426ad3e7` |
| 5 | 6 | `A` | 60 min Power Zone Endurance Ride | Ben Alldis | 2022-05-13 15:00 | `5f660f9700ec47599b51dead06fd2a53` |
| 6 | 1 | `P` | 45 min Power Zone Ride | Christine D'Ercole | 2022-05-13 15:00 | `062920dde7574be3a5a32628bb11d10c` |
| 6 | 3 | `Q` | 45 min Power Zone Ride | Matt Wilpers | 2022-05-13 15:00 | `b119477c055044458b155d257ebd1bf8` |

**`J`'s `classId` is not yet known.** The link supplied with it repeated week 1
day 6's `7c55c9f4…`, while the screenshots plainly show a different class — Denis
Morton, 45 minutes, warm-up 11 and 33 of riding against Alldis's 13 and 46. A
stale clipboard. Caught only because the interval transcription disagreed with
the id, which is the argument for capturing both rather than trusting either
alone.

**Week 1 is complete.** All three landed where the week table put them, and all
three were already transcribed, so the interval data and the mapping corroborate
each other rather than resting on one reading.

### The class page carries a prose structure note, and it checks out

`K`'s page, under *More info*:

> 13-min warm up followed by six Z3 intervals in a descending step formation
> (7/7/5/5/3/3) with 4, 3 & 2 min Z2 recovery in between each.

The transcription reads `Z3 7:00 · Z2 4:00 · Z3 7:00 · Z2 4:00 · Z3 5:00 · Z2
3:00 · Z3 5:00 · Z2 3:00 · Z3 3:00 · Z2 2:00 · Z3 3:00` — six Z3 intervals at
7/7/5/5/3/3 with Z2 recoveries of 4/4/3/3/2. **Exactly the stated formation.**

Two things follow. This field is **independent corroboration** of a transcription
where it exists, and it is the only place the class states its own *intent* —
"descending step formation" is a design decision, not something recoverable from
the interval list. Worth capturing where present.

### The movement count is a completeness check, and it is free

The `Class plan` summary states a movement count per section — `Cycling, 11
Movements`. It has matched the transcribed interval count on every class so far:
`M` and `L` at 7, `J` and `K` at 11, `N` at 5.

**That is what catches a truncated screenshot**, which is the likeliest way this
transcription goes wrong: a scrolled capture that silently omits the last
interval still looks plausible and still sums to something. Check the count
before recording a class.

**The date is not an air date and is weak corroboration.** These two classes are
by different instructors and carry the *same* timestamp, `Fri 13/5/22 @ 15:00`,
which no pair of live classes can. Whatever the field is — added to the
programme, or something else — it does not order or identify anything. Recorded
because it is on the page, relied on for nothing.

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
| P | 45 Power Zone | 13 | 17:00 | — | — | **—** | 9:00 | 31:00 † |
| Q | 45 Power Zone | 13 | 7:00 | — | — | **17:59** | 6:01 | 31:00 |
| E | 45 Endurance | 10 | — | 8:00 | 26:00 | — | — | 34:00 |
| L | 45 Endurance | 10 | — | 6:01 | 27:59 | — | — | 34:00 |
| M | 45 Endurance | 11 | — | 9:00 | 24:00 | — | — | 33:00 |
| F | 45 Endurance | 12 | — | 9:00 | 23:00 | — | — | 32:00 |
| N | 45 Endurance | 12 | — | 6:00 | **26:00** | — | — | 32:00 |
| A | 60 Endurance | 11 | — | 11:00 | 37:00 | — | — | 48:00 |
| G | 60 Endurance | 12 | — | 10:01 | 36:59 | — | — | 47:00 |
| O | 60 Endurance | 12 | — | 11:00 | **35:59** | — | — | 46:59 |
| C | 60 Endurance | 13 | — | 12:58 | 32:58 | — | — | 45:56 |
| K | 60 Endurance | 13 | — | 16:00 | 30:00 | — | — | 46:00 |

**Three archetypes fall out, and one number holds across two of them.**

- **Endurance** — Z3 with Z2 float, no Z4 at all. The 45s run 23–28 minutes of
  Z3; the 60s run 30–37.
- **Threshold** (`B`, `J`, `H`) — 16–18 minutes of Z4, the balance in Z3, Z1 to
  recover. No Z5.
- **VO2** (`D`, `I`) — 15 minutes of Z4 *and* 5–7 of Z5, with much more Z1
  because the recoveries have to be real.

**That held for eleven classes and week 6 broke it.** `P` — week 6 day 1 —
contains **no Z4 at all**, and contains **Zone 6**, which nothing else does:

```
Z5 3:00 · Z1 3:00 · Z5 3:00 · Z1 3:00 · Z5 3:00 · Z1 3:00
Z6 1:00 · Z1 2:00 · Z6 1:00 · Z1 2:00 · Z6 1:00 · Z1 2:00 · Z6 1:00 · Z1 2:00 · Z6 1:00
```

Nine minutes of Z5 in three-minute blocks, then five one-minute Z6 efforts, and
**seventeen of its thirty-one riding minutes are Z1 recovery**. No Z2, no Z3, no
Z4. It is a fourth archetype — call it anaerobic — and it is not a variation on
the VO2 rides but a different session entirely.

Two claims recorded here as settled are therefore withdrawn:

- *"Zones 1 to 5 appear; 6 and 7 appear nowhere."* **False.** Z6 appears at week
  6 day 1. Any zone type must carry at least 1–6, and given Peloton's scale runs
  to 7, capping it at what has been observed would be building the type from a
  sample.
- *"Every 45-minute Power Zone ride holds Z4 at 15–18 minutes."* **False**, and
  it was the load-bearing generalisation of the whole archetype section. Eleven
  consecutive confirmations, then a counterexample.

The lesson is the same one this document has already recorded twice: a pattern
over the transcribed subset is a description of the subset. `P` is the twelfth of
twenty-five.

**`P` is the outlier rather than the new rule.** `Q`, the very next session, is
back at 17:59 of Z4. Of thirteen Power Zone rides mapped, exactly one has no Z4.

### Week 6 day 3 finishes each threshold block with a surge

`Q` is a structure nothing else uses — four Z4 blocks, each running straight into
a short Z5 effort with no recovery between them:

```
Z4 5:00 → Z5 1:00 · Z1 2:00
Z4 4:00 → Z5 2:00 · Z1 3:01
Z4 4:59 → Z5 1:00 · Z1 1:59
Z4 4:00 → Z5 2:01
```

The Z4 blocks descend 5/4/5/4 and the surges alternate 1/2/1/2. It is neither the
threshold archetype (`J`, `H`, `B` — Z4 with Z3 between) nor the VO2 one (`D`,
`I` — Z4 and Z5 kept apart by Z1). *More info* was not captured for this class,
so Peloton's own word for it is unknown and none is invented here.

### Week 6 is the hardest week, not week 7

Day 1 is the only session with Z6; day 3 is the only one welding Z5 onto the end
of every Z4 block; and day 6 is the programme's only `60 min Power Zone Ride` —
the one long ride that is not an endurance ride. Week 7 has the only `Max Ride`
and the only 90-minute ride, so it looks like the peak from the titles, but on
what is actually mapped week 6 asks for more.

**The warm-up lengthens as the session hardens.** Both week 6 classes run 13
minutes of warm-up against 31 of riding — the longest warm-up and shortest ride
of any 45-minute class. The endurance rides sit at 10–12 against 32–34. Small,
consistent, and it falls out of the data rather than being imposed on it.

**"Z3 early, Z5 late" was written here and is wrong.** It was inferred from the
archetypes alone, before any class was mapped, and the mapping killed it: `I` —
five-by-thirty-second Z5 bursts, twice over — is **week 3 day 1**. Z5 arrives in
the third week, not late. The claim was asserted over the numbers in the same
sentence that called it visible in them, which is the failure this document
otherwise exists to avoid. Nothing about the archetypes changes; what changes is
that their *order* is the mapping's to state, never the grouping's.

`I` is the only class using 30/30s — five Z5 efforts with four Z2 recoveries,
twice.

## Transcribed classes — the intervals

Fifteen of twenty-five, **most not yet matched to a week or day**; the operator is
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
| N | `Z3 8:00 · Z2 3:00 · Z3 10:00 · Z2 3:00 · Z3 8:00` |

### `60 min Power Zone Endurance Ride`

| key | intervals |
|---|---|
| A | `Z3 9:00 · Z2 3:00 · Z3 7:00 · Z2 3:00 · Z3 5:00 · Z2 2:00 · Z3 7:00 · Z2 3:00 · Z3 9:00` |
| G | `Z3 9:00 · Z2 3:00 · Z3 9:00 · Z2 3:00 · Z3 7:00 · Z2 2:02 · Z3 6:58 · Z2 1:59 · Z3 5:01` |
| O | `Z3 8:00 · Z2 3:00 · Z3 8:00 · Z2 3:01 · Z3 5:59 · Z2 2:00 · Z3 6:00 · Z2 1:59 · Z3 4:00 · Z2 1:00 · Z3 4:00` |
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

### What the mapping shows so far

| week | day 1 | day 3 | day 6 |
|---|---|---|---|
| 1 | `M` endurance | `L` endurance | `K` endurance 60 |
| 2 | `J` threshold, Z4 18:00 | `N` endurance | `O` endurance 60 |
| 3 | `I` VO2, Z5 5:00 in 30/30s | `H` threshold, Z4 15:59 | `G` endurance 60 |
| 4 | `F` endurance, Z3 23:00 | `E` endurance, Z3 26:00 | `C` endurance 60 ⚠ |
| 5 | `D` VO2, Z5 7:00 sustained | `B` threshold, Z4 17:00 | `A` endurance 60 |
| 6 | `P` **anaerobic — Z6, no Z4** | `Q` Z4 blocks with Z5 surges | — |

Base week, threshold at week 2, Z5 by week 3. Day 3 lags day 1 by a week in
character — endurance while day 1 is threshold, threshold while day 1 is VO2.

**Week 4 day 1 is a real deload, and its shape says so.** `F` runs 23:00 of Z3
against week 1 day 1's 24:00 and week 1 day 3's 27:59 — less aerobic work than
the base week, in the middle of the programme. Its Z3 blocks also run
`3/5/7/5/3`, a pyramid up and back down, where every day-6 ride descends. That is
the only pyramid transcribed so far.

### The threshold rides do not progress, and that is worth recording

Three are mapped, and Z4 does not climb:

| | Z4 total | Z3 | Z1 | Z4 block sequence |
|---|---|---|---|---|
| week 2 day 1 — `J` | 18:00 | 12:00 | 3:00 | 4/3/2 · 4/3/2 |
| week 3 day 3 — `H` | 15:59 | 9:59 | 6:00 | 2/2 · 4/4 · 2/2 |
| week 5 day 3 — `B` | 17:00 | 10:00 | 6:00 | 3/2 · 4/3 · 3/2 |

**The most Z4 in the programme so far is week 2's**, and the block sequences share
no shape — one descends twice, one pyramids, one does neither. There may be no
threshold progression at all; the climb may live entirely in day 6's aerobic work
and day 1's Z5. Recorded as an absence so that a later session does not invent a
pattern here, and left open because three of eight weeks is thin evidence either
way.

### The Z5 work goes from bursts to blocks

Two VO2 sessions are mapped, both on day 1, and Z4 is 15:00 in each:

| | Z5 shape | Z5 total | Z1 recovery |
|---|---|---|---|
| week 3 — `I` | ten 0:30 bursts, in two blocks of five | 5:00 | 8:00 |
| week 5 — `D` | three sustained blocks, 2:00 / 3:00 / 2:00 | 7:00 | 10:00 |

**Same total riding time, same Z4, and the Z5 changes character rather than
merely growing** — thirty-second efforts against two- and three-minute ones.
Two minutes more Z5, bought with two minutes more Z1 to recover in. Short bursts
first, sustained blocks later, which is the standard way in to VO2 work and here
it is the programme's own ordering rather than a reading of the archetypes.

### Peloton names the formations, and there are three

Every word below is the class's own, from *More info* — not a reading imposed
here:

| formation | example | shape |
|---|---|---|
| descending step | `7/7/5/5/3/3` | hardest first, easing throughout |
| pyramid | `5/7/9/7/5` | builds to one peak, releases both ends |
| inverted pyramid | `9/7/5/7/9` | hard at both ends, easiest in the middle |

**Day 6 runs all three, in that order**: descending for the three build weeks,
pyramid for the week 4 deload, inverted pyramid at week 5. So the long ride
progresses in *shape* as well as in volume — and the shapes are not
interchangeable. A descending step spends its freshness immediately; an inverted
pyramid demands a hard effort after forty minutes of work, which is a different
demand at the same Z3 total.

**This vocabulary is Peloton's and stays out of the domain.** A formation is a
*description* of an interval sequence and is fully derivable from it, so storing
one would be keeping a second copy of something already recorded — and it would
put a vendor's taxonomy inside the model, which is the mistake this document
opens by correcting. Useful for reading the programme; never a field.

### The deload week is a pyramid, and that is its signature

**Among the *endurance* rides, week 4 pyramids where every build week descends.**
`F` on day 1 runs `3/5/7/5/3`; `C` on day 6 runs `5/7/9/7/5` and says so itself —
*"five Z3 intervals in a pyramid formation (5/7/9/7/5)"*. Every endurance ride
that states a formation is otherwise a descending step: `7/7/5/5/3/3`,
`8/8/6/6/4/4`, `9/9/7/7/5`.

**Scope matters and an earlier draft overreached.** It said these were "the only
two pyramids in anything transcribed", which is false: `H` — week 3 day 3, a
build week — runs its Z4 blocks at `2/2/4/4/2/2`, a pyramid by any reading. The
claim holds for the Z3 formations of endurance rides, which is where Peloton
itself uses the word. It does not hold across the Power Zone rides, and nothing
here has established that a Z4 block sequence and a Z3 interval formation are
the same kind of object.

A descending formation front-loads the hardest work while fresh. A pyramid peaks
once in the middle and releases at both ends. **The week's shape and the week's
purpose agree**, and it is the class's own word for it rather than a reading
imposed here.

Day 6 deloads on volume too: Z3 falls from `G`'s 36:59 to 32:58, and recovery
rises from 10:00 to 12:58 — the progression reversing for one week.

**Week 4 deloads by removing intensity, not volume.** Days 1 and 3 are both
endurance — no Z4 or Z5 anywhere — while `E` still runs 26:00 of Z3, *more* than
`F`'s 23:00 and more than week 1 day 1. The aerobic work is held and the hard
work is taken away.

### Days 1 and 3 are not two hard rides a week

Worth stating plainly, because the opposite was said in conversation before the
mapping existed and it was wrong. It came from reading `45 Power Zone` off the
week table for both days; two of those were obscured titles that have since
resolved to *Endurance* rides.

| week | day 1 | day 3 |
|---|---|---|
| 1 | endurance | endurance |
| 2 | threshold | endurance |
| 3 | VO2 | threshold |
| 4 | endurance | endurance |

A base week, one quality day against one aerobic day, two quality days, then a
deload. Only weeks 3, 5 and 6 carry two quality sessions. **The case against
dropping day 6 rests on the progression day 6 carries, not on days 1 and 3 being
punishing.**

## The day-6 ride is where the progression lives

Two consecutive day-6 rides are now mapped, and each states its own formation
under *More info*:

| week | Z3 intervals | formation | Z3 total | Z2 total | ride |
|---|---|---|---|---|---|
| 1 — `K` | 7/7/5/5/3/3 | descending step | 30:00 | 16:00 | 46:00 |
| 2 — `O` | 8/8/6/6/4/4 | descending step | 35:59 | 11:00 | 46:59 |
| 3 — `G` | 9/9/7/7/5 | descending | 36:59 | 10:00 | 46:59 |
| 4 — `C` | 5/7/9/7/5 | **pyramid** | 32:58 | 12:58 | 45:56 |
| 5 — `A` | 9/7/5/7/9 | **inverted pyramid** | 37:00 | 11:00 | 48:00 |

**Work climbs, recovery falls, and the ride length holds.** Thirty minutes of Z3
becomes thirty-seven across three weeks while Z2 recovery drops from sixteen
minutes to ten — inside a ride that never changes length. All three classes state
their own formation under *More info* and all three match the transcription.

An earlier reading here said "every Z3 block grows by a minute and every recovery
shrinks by one", which held for weeks 1 to 2 and broke at week 3: `G` drops to
*five* intervals rather than six. The invariant is not the arithmetic, it is
work up and recovery down at constant duration.

**This is the concrete answer to what dropping day 6 costs.** The progression the
programme runs on its long ride is a deliberate, week-by-week compression of
recovery against lengthening work — the thing that builds the aerobic base an FTP
is drawn from. It is not "a longer ride"; it is the only progressive element on
that day, and days 1 and 3 do not carry it.

Stated as fact, not advice. The choice is the operator's.

## Week 4 day 6 cannot be taken

`C` shows **`Unavailable`** rather than `Start` — the only class of the fifteen
transcribed that does. It is week 4 day 6, so if the operator takes day 6 at all,
one week of the eight has no long ride available to it.

Not diagnosed here: whether that is a licensing withdrawal, a regional
restriction, or something about the account. It has been `Unavailable` across two
separate captures an hour apart, so it is not a transient. A substitute would
have to be chosen by zone profile — `5/7/9/7/5`, 32:58 of Z3, 12:58 of Z2 — which
is precisely the search the published programme otherwise saves.

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
