# Roadmap

**Goal**: run the autumn on this tool — a coherent hybrid programme across the
gym and the bike, from week commencing Monday 14 September to week commencing
Monday 14 December 2026, prescribed session by session from an installed
binary.

**Written**: 2026-08-24, and revised whenever the plan moved. Rewritten
2026-09-04: the file had carried a `STALE` banner since 2026-09-03 and deferred
to a handover document, which is exactly the drift this file exists to prevent.
The 578-line version, with its completed step-by-step for August, is in git
history at `88cd715~1`.

The dates below are ordering, not estimates. **The constraint on this work is
how fast decisions get made, not how fast code gets written**, and that has been
true every week so far.

Decisions actually made live in `docs/decisions/`; this is the plan, not the
record. Session context — what a cold session needs to not re-derive yesterday —
lives in the current handover, `docs/handover-2026-09-03.md`, which is amended in
place rather than replaced.

---

## Now

**The autumn's gym side authors, tiles and prescribes.** Proven against a copy of
`local.db` on 2026-09-04, not argued:

```text
autumn-entry-test  test  2026-09-14  1 week   → 2026-09-20
sbs-1              sbs   2026-09-21  4 weeks  → 2026-10-18
sbs-2              sbs   2026-10-19  4 weeks  → 2026-11-15
sbs-3              sbs   2026-11-16  4 weeks  → 2026-12-13
```

**Both disciplines open with a test microcycle and run three mesocycles of
four** (0034). Thirteen weeks against thirteen, starting the same day:

```text
              week 1        weeks 2-13
gym       entry test    3 SBS cycles of 4
cycling   FTP test      3 mesocycles of 4
```

**Both disciplines prescribe a session today, and both from the store.** The gym
runs the whole loop — authored programme in the store, prescription, delivery to
Hevy. `cycling next` takes no arguments and does the same: `fitness plan` authors
the four cycling programmes, and `next` prints the session from the rows and then
puts it in the Peloton stack with its cool-down ride.

**The cycling record derives, and the entity is the session.** `fitness
normalise peloton.rides` fills the normalised layer with 150 cycling sessions —
144 rides and 6 FTP tests — from 903 landed records, with 357,969 samples
(issue #101, 2026-09-08). The FTP work below now has a performed record to read
rather than an assertion to take, and reads it as *tests* rather than as rides
it would have to recognise.

**A session, not a ride, and that is a rule rather than a cycling detail**
(constitution 3.2.0). Peloton files a session as two or three workouts — a
warm-up, a ride, a cool-down — and *"we would never consider these to be two
separate things that could be planned separately but Peloton does split them"*.
The same is true of the gym: the operator split single sessions across several
Hevy routines so he could compose them — 21 of his 140 training days carry more
than one record, three of them four — and `GymWorkout` called that canonical-
layer work. **Done on 2026-09-08** (#104, PR 105), so the rule now holds for both
disciplines: `fitness normalise hevy.workouts` writes 140 sessions from 167
records, and `compare` stops refusing those 21 days as an ambiguous day.

#104 had deferred this past 14 September over a boundary question that turned out
not to exist. It reported the same-day gaps as 7 to 46 minutes, measured start to
*start* — counting the previous workout as part of the gap. End to start they run
3 seconds to 8m20s, with 34 hours to the next day, which is what
`docs/gym-workout-domain-model.md` had said all along. Cycling's thirty minutes
carried over unchanged.

**What is missing is everything that joins them, and one gap on the cycling
side:**

- ~~Cycling has no authored programme~~ — **authored on 2026-09-05** (issue #55,
  migration 0023). Four cycling programmes are written by `fitness plan`: the FTP
  test week, then the three mesocycles, each with its rides, its zone plan
  and the class ids they are ridden at. **The test week is a published programme
  of its own** — *Power Zone test*, a copy of Build's fifth microcycle — for the
  reason the gym's entry test is a `test` programme rather than "SBS µ4": the
  operator, 2026-09-05, *"even though it's exactly the same classes, it's a
  separate thing"*. Without it the autumn authored Build µ5 twice, five weeks
  apart, with nothing to tell the two apart. Its own tables rather than a fifth
  `programme.template`, because the succession rule refuses two programmes
  covering one day and cycling covers the gym's days on purpose. The
  screenshot-transcribed Peak seed is deleted.
- ~~One of Peloton's four programmes is transcribed~~ — **Build was read from the
  Peloton API on 2026-09-05** (`docs/cycling-build-your-power-zones.md`, decisions 0032
  and 0033). Peak and Build are both in hand; Base is not, and is needed only if
  that pairing is chosen. Class *content* is now fetched rather than transcribed;
  the programme *skeleton* still is not available and remains the operator's.
- **No planner.** Nothing takes a span, the providers, the primary lift and a
  session count per discipline and returns the arrangements that cohere.
- **No fatigue coherence exists in the code.** `fatigue` appears five times and
  every one is about ordering exercises *within* a gym session by quality.
  Nothing weighs a cycling week against a gym week.

So: both disciplines can prescribe, neither can be planned, and nothing weighs
one against the other. **The gym loop is the deepest thing built; the hybrid
programming that the tool exists for is step 6.**

**Waiting on the operator**: one thing, and it blocks nothing. Whether 0018's
"spacing rule" is the same as *"a full rest day before the hardest gym session"*.

**PR #53 is open** and carries everything from 3–4 September.

**`plan` writes half of what it prints.** The cycling side is authored; the gym
side is still `fitness programme add`. Composing the gym wizard into `plan` is
issue #73, and the operator settled on 2026-09-05 that the half-written
intermediate state is fine rather than waiting for both.

## Order

**1. Remove TOML.** ~~The programme half~~ — **done 2026-09-06** (#61). The
document reader and writer are deleted, 1,723 lines, and the four pieces of
machinery the survey named went with them rather than being ported: eight
`refuse_unused` checks are unrepresentable, inherited fills have nothing to
inherit from, stated interruptions have nowhere to be stated, and a `[parameters]`
section has no document to sit in. `fitness programme add` asks and writes to the
store; there is no path, no `--into` and no file. What the operator answers is a
`domain::prescription::Authored`, and the assembly is `authored::programme`.

**What is left of it is `credentials.rs` and `settings.rs`**, and where
credentials go is open rather than settled on the keystore — see #61's comment
and the tail of `docs/removing-toml.md`. Settled 2026-09-03 and widened
2026-09-06: settings to the database, credentials somewhere safe — *"it could
also be the database"*.

This was first because everything after it would otherwise be written twice, and
#86 is the first thing that would have: relevelling `Programme` into a plan, a
programme and a mesocycle would have meant relevelling a document reader with a
known expiry date.

**1a. A plan holds programmes** (#86) — **done on 2026-09-07**, on
`refactor/a-plan-holds-programmes`, and green on the whole gate. The operator's
hierarchy is `macrocycle → plan → programme → mesocycle → microcycle → session`,
and the store had four rows per discipline and no plan at all.

Every rung of it now exists: the domain renames (`Mesocycle`, `Progression`,
`BlockPeriodisation`, `PublishedProgramme`, `CyclingMesocycle`),
`Progression::Provided` in place of `Sbs`, `domain::provider` — who published a
programme and which of its microcycles a mesocycle took — `Plan`, `Programme<M>`,
`Span` and `PlanWindow` in `domain::plan`, the `PlanStore` and `PlanAuthor`
ports, and migration 0024 rebuilding the authored side around a `plan` table.

**There is no table for the programme rung**, deliberately: the gym programme
*is* the `gym_mesocycle` rows under a plan, in `ordinal` order.

**The rows went rather than being carried.** The operator authorised that on
2026-09-06 — *"there's nothing that can't be reconstructed after the fact"* — and
**the authorisation expires when the autumn starts** (§ 12): a migration after
14 September carries its rows or does not land. 0024 was applied to a fresh
store and to copies of both beta stores before it was committed.

**#73 closed with it, on the second attempt.** `fitness plan` now authors both
programmes: the gym questions are asked once for the whole plan through
`wizard::gym_side`, and four gym mesocycles are laid out from the answers rather
than typed in over three months.

**What had blocked it was the anchor, not the wizard**, and nobody had written
that down. Authoring the gym side whole means giving every mesocycle a number its
loads are shares of, and the cycles beginning 19 October and 16 November open
from week-4 maxima nobody has lifted yet. So `Anchoring` is `Stated | Inherited`
(0025): a cycle may defer to whatever the one before it measured, resolved
against the record when a session is asked for and refused where the record is
silent. It is the move the chart already makes inside a cycle, one level up.

**Names settled with the operator on 2026-09-06**, and worth not re-deriving:
the gym programme is *Squat 2x Int* provided by Stronger By Science; the cycling
side is *Build Your Power Zones* entry test, then Build micros 1-2-4-5, Peak
micros 1-2-3-4 and Peak micros 5-6-7-8 — **which settles the pairing** the file
below still calls a programming choice. *Power Zone Build* was a wrong
transcription throughout — Peloton calls it **Build Your Power Zones** — and was
corrected everywhere on 2026-09-06.
`Progression::Sbs` was renamed because a provider is a relation rather than a
rung — *"Peloton and SBS are providers of programmes… within our tool, those same
programmes are providers of mesocycles"*.

**2. 0027's deletions** — `Entry`, `Anchor`, `declared_opening`,
`TestTarget::Declared`, and the anchor columns.

**3. 0028's split** — `Scales` stays a generation parameter; `WarmupStep`,
`BackOff`, `TopSetReps`, `ResetProtocol` and `AccessoryScheme` stop being
parameters and become facts about the world.

**4. The ordinal programme** (0018), then the allocator: pin, alternation,
spacing. **Remove `Prescribed::Autoregulated` while here** — it has had no
producer since 2026-09-04 and the operator settled on 2026-09-05 that it goes.
It is a migration.

**5. Transcribe the cycling programmes.** Base, Build and Peak were all read
from the Peloton API on 2026-09-05, which is every programme this tool will use.

**Discover is out of scope, and permanently.** The operator, 2026-09-05: *"it's
specifically design to introduce the concept of power zones to a new rider. it's
first week has 7 classes over 5 days, including a FTP warm and test pair on day
2. I don't think it's a programme we're going to be pulling from going
forwards."* Its ids were never asked for and should not be. It corroborates 0034
in passing — the introductory programme tests FTP almost first, because a zone is
a share of a number the rider does not yet have. Record what a provider answers when asked for four
microcycles. ~~As a **set** of options, not one (0029)~~ — 0036 settled that it
is a single answer, the lowest score, because a tie preserves no choice.

**A provider supplies mesocycles, not programmes** (0036), **and the entry test
is one of them** — one microcycle of two sessions, not a category of its own
(2026-09-06). Besides it there are five:

```text
base 1   µ1-2-3-4  by sessions 2+3     composition  6.0
base 2   µ5-6-7-8  by sessions 1+2                  1.1
build    µ1-2-4-5  by sessions 1+3                  5.4
peak 1   µ1-2-3-4  by sessions 1+3                  5.4
peak 2   µ5-6-7-8  by sessions 1+3                 14.6
```

`transcribe <skeleton> 4 2` computes them. The autumn needs three, and both
pairings 0034 admitted are three. ~~The cycling side also needs an authored
programme that can hold a test microcycle ahead of its periodisations~~ —
**built on 2026-09-05**, and it needed no template to do it: a cycling programme
is microcycles of rides, and the test microcycle is a one-microcycle programme
whose Sunday ride carries a duration and no zone.

~~**6. Writing to the Peloton stack** (#70)~~ — **done 2026-09-06**, and it
needed three things nobody had: the stack takes a base64 join token rather than a
ride id (#70), a session names its own cool-down ride by query (#79), and the
access token survives between runs so a write costs no login (#54). `cycling
next` delivers, as `gym next` does.

**7. The planner, the span view, and `fitness next`.** The tool takes a span, the
providers, the primary lift and a session count per discipline per microcycle,
and returns every arrangement whose fatigue profiles cohere. This is the
deliverable the other six exist for.

### What changed the order, and when

- **0024** made a published programme something transcribed rather than derived,
  which is why the SBS chart is a table and not a formula.
- **0026 → 0029** moved the bounded context from the published programme to the
  **provider**, and made the planner ask a provider for a shape rather than
  compute one. That is what put the planner last: it cannot be written until
  there are two providers to ask.
- **0027** dissolved the old open question *"what anchors a programme that
  follows another?"* rather than answering it. A programme is a shape; the
  numbers come from the record.
- **0034** made coherence a constraint rather than an objective: the planner
  admits or refuses an arrangement instead of ranking one. That shrinks step 6 —
  there is no scoring function to design — and it fixed Build at four
  microcycles from a direction unrelated to 0032's.
- **2026-09-04** added step 0 and finished it in the same day — see below.

## What 2026-09-04 changed

Nothing on the list above moved. What changed is that the gym side went from
"believed to work" to "seen working on the operator's data", and three faults
surfaced in the process. All of it is PR #53.

**The store could not hold an SBS cycle at all.** `Sbs` had existed in `domain`
since 0024, the document reader accepted `template = "sbs"` and `sbs_load`
prescribed from it — but `programme`'s `CHECK` named three templates and `sbs`
was not one. The autumn was unauthorable and nothing said so. Migration 0022.

**It was found by running the thing, not by reading it.** Every check was green
while this was true. The lesson is already a memory and is now also a fact about
this project: a green gate is not a user-visible improvement, and the first
person to try the actual command finds what no test asked.

**Three faults in one prescribed line**, all fixed: the ramp never asked for more
than four repetitions before a set of eight (0030); the top set withheld a target
the ramp was already built from; the back-offs claimed to be taken to failure,
which the chart does not ask.

**The rounding was losing ground (0031).** A departure from the published
arithmetic, whose own rule has the same flaw hidden behind a round worked
example. Flag it rather than "fix" it back.

## Nothing is open

Four things sat here as "open questions" until 2026-09-05, when the operator
pointed out that none of them was one. **An open question is only open if
resolving it unblocks something** — the criterion is in `CLAUDE.md` and it
applies to `docs/decisions/` as well.

- **Which cycling pairing** — `base 1, base 2, build` or `build, peak 1, peak 2`.
  *"that's not a question, that's a programming choice."* 0034 admits both. The
  one input the tool can offer: Base carries no FTP test of its own, so the first
  re-anchors the zones at weeks 1 and 13 where the second does it at 1, 5 and 13.
- **Is the spacing rule 0018's?** 0018's own worked example answers it — Monday
  gym, Wednesday cycling, Friday gym, Sunday cycling leaves a clear day before
  the heavy session.
- **Does `--timezone` survive as a per-run override?** The scheduler already owns
  the zone: `schedule.rs` resolves arithmetic through the calendar's IANA zone
  and a test pins that it uses the calendar's rather than the machine's. Whether
  the CLI keeps a flag once the store holds the setting is a consequence.
- **Does `Prescribed::Autoregulated` come out?** Yes — *"we're not using it."*
  It is a migration, so it is a **task** and is in the order above, not here.

Answered and kept here only because a session may go looking: credentials and
settings (2026-09-03, no TOML); zone minimums are independent floors
(2026-09-03); what a rep-max day prescribes (2026-09-04, one set at a stated
load); Peloton is reachable and serves class content and the performed record but
not programme structure (2026-09-05, 0033).

**Due, not merely reopened** (0033 reopened it, 0034 dates it): the FTP work.
The block cannot start without a fresh value — every zone in twelve weeks of
prescription is a share of a number whose most recent reading is nearly eight
weeks stale by 14 September. It was taken off the list on 2026-09-03
because the need arrived with Peloton ingestion; Peloton ingestion now exists,
and the record holds six effect-dated FTP values — 143, 183, 199, 174, 155 and
**172 on 2026-07-22**, each the twenty-minute test's average output × 0.95. An
*asserted* FTP is no longer the only path and is probably the wrong one.

## Deferred, and none of it on the critical path

- **The zone read by date at derivation.** The § 13 defect is real — change the
  zone, re-normalise, and every workout's wall clock is rewritten — but it bites
  only if the operator trains in another zone. It should land before it can bite.
- ~~The cool-down ride~~ — **built 2026-09-06** (#79, #82). Found by query rather
  than by a table, because what the operator rides is the most recent one; four
  of the twelve instructors publish none and fall back to Matt Wilpers.
- **Slot amendments** — needed the next time equipment moves, not before.
- **The Peloton class library, cached** (#94). `fitness plan` fetches sixty-five
  classes on every authoring and keeps none of them.

## Deliberately out of scope

- **A third data source.** Withings body weight is the strongest candidate — the
  degenerate entity § II.3 names, and it would exercise § 6's comparability
  classes across *sources*, which nothing has yet. It competes for the same weeks
  and does not help the operator train.

  This used to say "a second data source", and that the architecture "has never
  been tested against a second source". Peloton is the second, and as of
  2026-09-08 it derives: the normalised layer holds two entities, the ports are
  generic over which, and the vocabulary the derivation needs is no longer inside
  `domain::gym`. What is still untested is a source that observes something
  another source already observes.
- **The macro layer** — nutrition, the family calendar, and anything that
  *decides* how a week is spent. Slots are recorded **and allocated**; what waits
  is choosing the split.

## Risks

- **A migration after 14 September carries its rows or does not land** (§ 12).
  Raw landing re-fetches from Hevy and everything derived rebuilds; programmes
  and prescriptions do not. `local.db` is the **beta** store and stays
  disposable — migrate it or start fresh without ceremony. The autumn runs on
  the XDG store, which is empty until it is authored into, and from then on 0024
  is not the precedent to copy: it dropped sixteen tables and carried nothing.
  Nothing in `nix flake check` enforces this.

  **A backup is not the remedy, and #68 was closed as a non-issue on
  2026-09-07.** Production authors a plan once and then performs it, amending a
  schedule or a slot; the repeated re-authoring that made the store look fragile
  was beta testing.
- **`fitness deliver` names one sink and there are now two** (#67). The flat
  command compiles in `hevy` where `gym next` and `cycling next` each reach their
  own; a cycling `KnownDiscipline` stays blocked on there being no Peloton
  *source* to collect from.
- **Cutting the release at the wrong moment.** 1.0.0 is reserved for the version
  that runs the autumn, and crossing it is a release choice rather than a
  consequence of a breaking change. On 2026-08-26 a release PR merged because
  conventional commits had piled up, tagging `v1.0.0` on a tool that could not
  author the autumn correctly; it was backed out in #37. **Too early is as real a
  failure as too late, and cheaper to make.** Once the block is running the
  operator should be on the pinned release rather than tracking `main`.

## Two things a new session should read first

- `docs/constitution.md`, which governs. Short and binding.
- `CLAUDE.md`, for the way of working. Spec Kit is retired and `specs/` is
  deleted — see decision 0024. Do not restore it, do not cite it.

And two framings settled in conversation that are not otherwise written down:

**Programming is a function of stated inputs, not a consulter of sources.** The
tool is told to generate a programme from x to y including absences a and b. The
brain that decides what the absences *are* sits above the gym level, because it
also weighs cycling, nutrition and the family calendar.

**Recording a fact is not coordinating**, so the line is not gym-versus-macro but
fact-versus-planning. Recording that the operator can train on Monday evening is
data this tool should hold.

**The allocation is on the fact side** — revised 2026-08-25. *Deciding* the split
between gym and bike is planning and still waits; *which discipline holds Monday
evening* is a fact the schedule has to hold, because an alteration can move it. A
trip where the hotel gym is only free at the weekend turns two weekday evenings
into a Saturday morning, and the allocation has to move with them. So
`Diary::unavailable` takes a discipline and reads the allocation, rather than
taking a set of slots somebody else kept in step.
