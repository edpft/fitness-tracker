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
| 8 | 45 Endurance — D'Ercole | 45 **Endurance** — Morton | **10 min FTP Warm Up + FTP test** — Wilpers |

Three day-3 titles were hidden behind the `Join program` button. **All three are
resolved by the mapping, and all three are Endurance rides** — weeks 2, 4 and 8.
Every title above was read directly or established by a class the operator
matched to the slot.

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
| 6 | 6 | `R` | 60 min Power Zone Ride | Christine D'Ercole | **2023-04-21 16:00** | `ae5058e68cf045058bbf405b3e115dda` |
| 7 | 1 | `S` | 45 min Power Zone **Max** Ride | Ben Alldis | 2022-05-13 15:00 | `251e957464f74530937782a6080eecf9` |
| 7 | 3 | `T` | 45 min Power Zone Ride | Christine D'Ercole | 2022-05-13 15:00 | `57af0cb0dfb44abba73af9798e312d2d` |
| 7 | 6 | `U` | **90** min Power Zone Endurance Ride | Matt Wilpers | 2022-05-13 15:00 | `597a32a0c58a4625b5d9299daffb2e05` |
| 8 | 1 | `V` | 45 min Power Zone Endurance Ride | Christine D'Ercole | 2022-05-13 15:00 | `5833bec716724236bd9d12730ff29776` |
| 8 | 3 | `W` | 45 min Power Zone **Endurance** Ride | Denis Morton | 2022-05-13 15:00 | `a85ab401308f42268394273971f5468c` |
| 8 | 6a | `X` | 10 min FTP Warm Up Ride | Matt Wilpers | 2022-05-13 15:00 | `f3474128dec54bbcb7f3775161e4f45e` |
| 8 | 6b | `Y` | 20 min **FTP Test** Ride | Matt Wilpers | 2022-05-13 15:00 | `67578c4e666046469a20987ccf70ee5f` |

**All twenty-five classes are mapped.**

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

**The date is not an air date, and it does not identify a class.** Sixteen of the
seventeen mapped classes carry the *identical* timestamp `Fri 13/5/22 @ 15:00`,
across five instructors — which no set of live classes can share. The seventeenth,
`R` at week 6 day 6, reads `Fri 21/4/23 @ 16:00`.

So the field varies, but almost never, and a value shared by sixteen classes
distinguishes nothing. Best guess is that it dates the *programme's* publication
and that `R` was substituted in later — which would also explain why week 6 day 6
is the one long ride that is not an endurance ride. **A guess, recorded as one.**
Relied on for nothing either way.

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
| R | 60 Power Zone | 13 | 8:00 | — | 22:00 | **16:00** | — | 46:00 |
| S | 45 Max | 12 | 14:00 | 2:30 | — | **9:00** | 1:30 | 32:00 ‡ |
| T | 45 Power Zone | 13 | 8:31 | 4:59 | — | **7:00** | 7:02 | 31:00 § |
| U | 90 Endurance | 13 | — | 20:00 | **56:00** | — | — | 76:00 |
| V | 45 Endurance | 13 | — | 8:00 | **23:01** | — | — | 31:01 |
| W | 45 Endurance | 13 | — | 12:00 | **19:00** | — | — | 31:00 |

§ `T` also carries **Z6 3:28**.

‡ `S` also carries **Z6 3:00 and Z7 2:00** — the only two columns this table
has never needed. Its Z1 of 14:00 is 44% of the ride.
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

- *"Zones 1 to 5 appear; 6 and 7 appear nowhere."* **False twice over.** Z6
  appears at week 6 day 1 and **Z7 at week 7 day 1**. All seven zones are now
  observed, and the zone type carries 1–7 as a fact rather than as caution.
  Withdrawing this in two stages is itself the point: the first correction
  guessed the type "must carry at least 1–6" and hedged about 7 — the hedge was
  right, and had the type been sized to the evidence at either moment it would
  have been wrong.
- *"Every 45-minute Power Zone ride holds Z4 at 15–18 minutes."* **False**, and
  it was the load-bearing generalisation of the whole archetype section. Eleven
  consecutive confirmations, then a counterexample.

The lesson is the same one this document has already recorded twice: a pattern
over the transcribed subset is a description of the subset. `P` is the twelfth of
twenty-five.

**`P` is the outlier rather than the new rule.** `Q` is back at 17:59 of Z4 and
`R` — a *sixty*-minute Power Zone ride — sits at 16:00. Of fourteen Power Zone
rides mapped, exactly one has no Z4, and the 15–18 minute band has now held
across both class lengths.

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

### Week 6 day 6 is a threshold ride, and states its own sets

`R` is the programme's only `60 min Power Zone Ride` — the sole long ride that is
not an endurance ride. Its *More info*:

> 13-min. warm up followed by 3 sets of alternating Z3 and Z4 intervals
> (4/3/4/3, 3/2/3/2, 4/3/4/3) with 4 min. of Z1 recovery in between each.

The transcription matches exactly. Note the sets dip in the middle — big, small,
big — which is the shape `A` carries at interval level and Peloton calls an
inverted pyramid. **It does not use that word here**, so the shape is noted and
the label is not borrowed.

### The Max Ride is a fifth archetype, and it reaches Z7

`S`, week 7 day 1, is the programme's only `Max Ride` and the only class touching
Zone 7. Thirty-three movements — half again as many as anything else:

```
⟨Z6 0:30 · Z4 3:00 · Z7 0:15 · Z1 3:15⟩ × 3
⟨Z5 0:30 · Z2 0:30⟩ × 3 · ⟨Z6 0:30 · Z2 0:30⟩ × 2 · Z6 0:30
Z1 3:15
⟨Z7 0:15 · Z1 0:15⟩ × 4 · Z7 0:15
```

Three threshold blocks, each opened by a half-minute at Z6 and **closed by a
fifteen-second sprint at Z7**; then a 30/30 ladder climbing Z5 into Z6; then eight
fifteen-second Z7 sprints against fifteen-second recoveries. Z7 totals two
minutes, in eight bites. Z1 totals fourteen — **44% of the ride is recovery**,
against 55% in `P` and under 20% in every threshold class.

**This one needed stitching from two overlapping captures**, and both checks held:
the reconstruction is 33 movements against a stated 33, and sums to 32:00 against
a stated 32. Neither check would have survived a dropped or duplicated interval
across the seam, which is exactly what they are for.

### Week 7 day 3 climbs the zones twice

`T` runs two sets, each ascending Z4 → Z5 → Z6 while the intervals shorten:

```
set 1   Z4 4:00 · ⟨Z5 1:00 · Z2 1:00⟩×3 · Z5 1:01 · ⟨Z6 0:30 · Z1 0:30⟩×3 · Z6 0:30
set 2   Z4 3:00 · ⟨Z5 1:00 · Z2 1:00⟩×2 · Z5 0:59 · ⟨Z6 0:29 · Z1 0:30⟩×2 · Z6 0:30
```

Four minutes at Z4, then minutes at Z5, then half-minutes at Z6 — **the zone
rises exactly as the interval shortens**, and the second set is the first with one
repetition removed from each rung. Thirty-one movements, and the count matches.

### Week 6 or week 7 — resolved, and they peak at different things

An earlier section here argued week 6 was the hardest week, on the evidence then
available. With week 7 days 1 and 3 mapped the honest answer is that the question
was badly posed:

| | Z4 | Z5 | Z6 | Z7 |
|---|---|---|---|---|
| week 6, days 1+3 | 17:59 | 15:01 | 5:00 | — |
| week 7, days 1+3 | 16:00 | 8:32 | 6:28 | **2:00** |

**Week 6 peaks on volume at threshold and VO2; week 7 peaks on intensity** — and
once day 6 is counted, week 7 also peaks on total work:

| | riding time | Z3 | Z4 | Z5 | Z6 | Z7 |
|---|---|---|---|---|---|---|
| week 6 | 108:00 | 22:00 | **33:59** | **15:01** | 5:00 | — |
| week 7 | 139:00 | **56:00** | 16:00 | 8:32 | 6:28 | **2:00** |

Week 7 rides half an hour longer, does more than twice the Z3, and is the only
week reaching Z7. Week 6 does more than twice the Z4 and nearly twice the Z5.
**Week 7 is the peak week**; week 6 is the hardest concentration of threshold and
VO2 work. The earlier claim that week 6 was simply the hardest week is withdrawn.

Now that all three days are mapped: day 1 is the only session with Z6, day 3 the
only one welding Z5 onto the end of every Z4 block, and day 6 the only long ride
that is not endurance — sixteen minutes of Z4 on top of the other two. Week 7 has the only `Max Ride`
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
| 6 | `P` **anaerobic — Z6, no Z4** | `Q` Z4 blocks with Z5 surges | `R` **threshold 60**, Z4 16:00 |
| 7 | `S` **Max — Z7**, 33 movements | `T` Z4→Z5→Z6 ladder | `U` **90 min**, Z3 56:00 |
| 8 | `V` endurance, Z3 23:01 | `W` endurance, **Z3 19:00** | `X` warm-up, then `Y` the **retest** |

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

### The taper and the deload are the same session twice

`V` (week 8 day 1) and `F` (week 4 day 1) are near-identical, and both are
Christine D'Ercole:

| | formation | Z3 | Z2 | ride |
|---|---|---|---|---|
| week 4 — `F` | `3/5/7/5/3` | 23:00 | 9:00 | 32:00 |
| week 8 — `V` | `7/9/7` | 23:01 | 8:00 | 31:01 |

Twenty-three minutes of Z3 in each, one second apart, both pyramids, both by the
same instructor. `V` reaches it in three intervals rather than five and takes the
programme's longest Z2 recoveries — four minutes each — which is what separates a
taper from a deload: the same aerobic dose, fewer efforts, more rest between
them.

**Week 8 goes lighter still on day 3.** `W` runs **19:00 of Z3 against 12:00 of
Z2** — the least Z3 and the most Z2 of any 45-minute ride anywhere in the
programme. So the taper deepens across the week: 23:01, then 19:00, then the
test.

**Which sharpens the day-6 problem considerably.** In week 8 the operator's two
chosen sessions are the two easiest rides of the whole eight weeks, and the only
thing that measures anything is the day he is dropping. The final week would be
two recovery rides and nothing else.

### The formation tally, without an inference attached

Five pyramids are now transcribed: week 4 days 1 and 6, week 7 day 6, week 8 days
1 and 3. Four of the five sit in the deload and the taper; the fifth is the
volume peak. Descending steps hold weeks 1 to 3; the single inverted pyramid is
week 5.

**No claim is made about what that means.** Three earlier attempts to tie
formation to purpose were each withdrawn, the last of them by `U`. The tally is
recorded because it is a fact about the data; the pattern it suggests has already
been wrong once at four-out-of-five confidence.

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

**The formation carries no meaning about the week, and this document tried three
times to make it.** The successive claims were: week 4 pyramids where build weeks
descend; then, narrowed, that this holds among endurance rides; and both are now
dead. `U` — week 7 day 6, the programme's **volume peak** at 56 minutes of Z3 —
states *"five blocks of 2 x Z3 intervals in pyramid formation"*. The same
formation as week 4's deload, at nearly twice the work.

So the formations sort like this, and no further:

| formation | appears in |
|---|---|
| descending step | weeks 1, 2, 3 — day 6 |
| pyramid | week 4 deload (days 1 *and* 6), week 7 volume peak |
| inverted pyramid | week 5 day 6 |

**A formation describes the arrangement of the work, not its purpose.** Volume
and zone say what a week is for; shape says how it is laid out. Attaching the two
was an inference each time, and it failed each time — recorded at length because
the failure repeated after being corrected once.

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
| 6 — `R` | 4/3/4/3 · 3/2/3/2 · 4/3/4/3 | *(threshold, Z3+Z4)* | 22:00 | — | 46:00 |
| 7 — `U` | 2×4 / 2×6 / 2×8 / 2×6 / 2×4 | **pyramid** | 56:00 | 20:00 | 76:00 |

**Week 7's long ride is the volume peak by a distance** — 56 minutes of Z3 against
a previous high of 37, in a 90-minute class. Week 6's breaks the series
differently, being the only long ride that is not endurance at all.

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

## The FTP test prescribes a duration and no zone, and that is the point

`Y` — the retest — has a `Class plan` of **`Cycling · 20 mins` with no movement
count and no breakdown at all.** No warm-up section (that is `X`, a separate
class), no cool-down, and no intervals. Difficulty 9.6/10, the highest in the
programme. Its *More info*:

> This workout is all about realizing the fitness gains you have made since
> starting this program. Maximize your average output in this test and compare
> it to your previous test.

**It cannot be expressed in zones, because it is what defines them.** A zone is a
share of FTP; this ride measures FTP. Prescribing it as "ride at Z5 for twenty
minutes" would be circular — the whole point is that the output is unknown until
it is ridden.

### Which is exactly the shape the gym side already has

Decision 0023 added a `WorkUp` variant to `WeekPlan`: **a repetition count and no
load**, because the load is discovered by working up rather than derived from a
maximum. `Y` is the same move in the other discipline: **a duration and no zone**,
because the intensity is discovered.

```
gym       WorkUp   reps,     no load    →  discovers the load
cycling   test     duration, no zone    →  discovers the output
```

Both sit at the end of their programme, and both measure the number every other
prescription in that programme is a share of — the 1RM for SBS's percentages,
FTP for Peloton's zones. **This is the strongest evidence so far that the two
disciplines are one model rather than two**, and it was not designed in: the gym
variant was written a week earlier for unrelated reasons and the cycling case
fell out of a transcription.

## A class need not have a ride at all

`X` — the FTP warm-up — has a `Class plan` consisting of **`Warm Up · 10 mins ·
6 Movements` and nothing else.** No `Cycling` section, no `Cool Down`. Its
*More info* reads:

> It's time to prepare your body to achieve a higher average output than you did
> in your last FTP test. Let's warm up!

Every other class in the programme is warm-up, then a zone-interval ride, then a
one-minute cool-down. **A session type carrying no prescribed intervals must
therefore be representable** — and a model that made the interval sequence
mandatory, or non-empty, would be unable to hold one of the twenty-five.

### And no warm-up anywhere is transcribed

`X` makes visible a gap that had gone unnoticed because it never mattered before.
Every class states its warm-up as a duration and a movement count — `13 mins ·
9 Movements` — and **the zone breakdown of that section was never expanded in any
capture.** Only `Cycling` was.

That is **10 to 13 minutes per class across twenty-four classes: on the order of
four and a half hours of riding with no zone data at all**, against roughly
seventeen hours that are fully transcribed. For most classes the omission is
tolerable — a warm-up is a warm-up. For `X` it is the entire session, so what is
recorded here is a duration and nothing else.

Left as a known hole rather than guessed at. Whether it is worth closing depends
on whether warm-up zones matter to what the tool computes; if the answer is that
a prescription states the whole session, it does.

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

## The three day-pairings, computed

The question the transcription existed to answer. Ride portions only —
warm-ups are untranscribed throughout, so all three columns understate equally.
Totals across the whole eight weeks:

| pairing | Z1 | Z2 | Z3 | Z4 | Z5 | Z6 | Z7 | ride | untimed |
|---|---|---|---|---|---|---|---|---|---|
| **days 1+3** | 79:31 | 69:30 | 200:59 | **114:58** | **35:33** | **11:28** | **2:00** | 513:59 | — |
| **days 1+6** | 60:00 | 113:29 | 332:57 | 73:00 | 22:30 | 8:00 | **2:00** | 641:56 | 30:00 |
| **days 3+6** | 35:31 | 117:59 | **369:54** | 73:58 | 13:03 | 3:28 | — | 643:53 | 30:00 |

*(All three days: Z3 451:55, Z4 130:58, ride 899:54.)*

**These are three different programmes, and the operator's phrasing was exact.**

- **Days 1+3** is the intensity programme. It carries **57% more Z4** than either
  alternative, nearly three times the Z5, and over three times the Z6 — in two
  fewer hours of riding. It is also the only pairing that misses the FTP retest.
- **Days 3+6** is its opposite: the most Z3 of any pairing, almost no Z5, and
  **no Z7 at all** — the Max Ride is on day 1, so that pairing never touches the
  programme's hardest session.
- **Days 1+6** is the balanced one. It keeps the Max Ride *and* the long-ride
  progression, sits within three minutes of days 3+6 on total riding time, and
  lands between the two on every zone above Z2.

**Days 1+6 also solves the week 8 problem for free**, because day 6 of week 8 is
the FTP retest. No exception has to be remembered.

Two costs, stated because they are real: days 1+6 gives up roughly 42 minutes of
Z4 against days 1+3, and week 4 day 6 is the class showing `Unavailable`, so one
of its eight long rides needs substituting.

**Not a recommendation — the operator has the numbers now, which is what he asked
for.** Note only that the ride totals are not comparable as time commitments: days
1+3 asks for 8h34m across eight weeks, the other two for about 10h42m.

## Open

**The class-to-session mapping** — the operator is supplying it explicitly.

**The remaining twelve class plans**, at least for the days actually taken.

**Which two days.** Days 1 and 3, 1 and 6, or 3 and 6 — three different
programmes, and once the mapping lands, three computable ones.
