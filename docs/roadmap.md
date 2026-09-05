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

**Both disciplines prescribe a session today.** The gym runs the whole loop —
authored programme in the store, prescription, delivery to Hevy. `cycling next`
prints a full session from the transcribed Peloton programme: warm-up, the
intervals in order, time in zone, cool-down and the class link.

**What is missing is everything that joins them, and two gaps on the cycling
side:**

- **Cycling has no authored programme.** `cycling next --start 2026-09-14` takes
  the start date as a flag every run. Nothing is stored — `cycling` appears in
  the migrations only as a `discipline` value on training slots. The gym authors
  and remembers; cycling recomputes from a flag.
- ~~One of Peloton's four programmes is transcribed~~ — **Build was read from the
  Peloton API on 2026-09-05** (`docs/cycling-power-zone-build.md`, decisions 0032
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

## Order

**1. Remove TOML.** `docs/removing-toml.md` has the survey. ~1,700 lines
deleted, and four pieces of machinery become unnecessary rather than needing to
be ported. Settled 2026-09-03: credentials go to the OS keystore, settings to the
database, **no TOML anywhere**. This is first because everything after it would
otherwise be written twice.

**2. 0027's deletions** — `Entry`, `Anchor`, `declared_opening`,
`TestTarget::Declared`, and the anchor columns.

**3. 0028's split** — `Scales` stays a generation parameter; `WarmupStep`,
`BackOff`, `TopSetReps`, `ResetProtocol` and `AccessoryScheme` stop being
parameters and become facts about the world.

**4. The ordinal programme** (0018), then the allocator: pin, alternation,
spacing.

**5. Transcribe the cycling programmes.** Base, Build and Peak were all read
from the Peloton API on 2026-09-05, which is every programme this tool will use.

**Discover is out of scope, and permanently.** The operator, 2026-09-05: *"it's
specifically design to introduce the concept of power zones to a new rider. it's
first week has 7 classes over 5 days, including a FTP warm and test pair on day
2. I don't think it's a programme we're going to be pulling from going
forwards."* Its ids were never asked for and should not be. It corroborates 0034
in passing — the introductory programme tests FTP almost first, because a zone is
a share of a number the rider does not yet have. Record what a provider answers when asked for four
microcycles as a **set** of options, not one (0029). **The cycling side also
needs an authored programme that can hold a `Test` microcycle ahead of its
periodisations**, the way the gym's already does (0016, 0034).

**6. The planner, the span view, and `fitness next`.** The tool takes a span, the
providers, the primary lift and a session count per discipline per microcycle,
and returns every arrangement whose fatigue profiles cohere. This is the
deliverable the other five exist for.

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

## Open questions

1. **Which cycling pairing** — Base (8) + Build (4), or Build (4) + Peak (8).
   **0034 checked both and admits both**, so the choice is now known to be
   unforced rather than merely unanswered: every gym test week lands on a cycling
   test or deload either way. Still the operator's judgement, and it gained one
   input it did not have — Base carries no FTP test of its own, so
   `test + Base(8) + Build(4)` re-anchors the zones at weeks 1 and 13 where
   `test + Build(4) + Peak(8)` does it at 1, 5 and 13. Eleven weeks on one FTP is
   the cost of the first. The hand-worked alignment of 2026-09-03 has now been
   computed rather than asserted.
2. **Does `--timezone` survive as a per-run override** once the store answers?
   Probably, and it should stop being *required*.
3. **Is the spacing rule 0018's?** The one thing waiting on the operator.
4. **Does `Prescribed::Autoregulated` come out?** It has had no producer since
   2026-09-04: every session now states a load. Removing it is a migration, so
   it waits until the model has clearly settled.

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
- **Slot amendments** — needed the next time equipment moves, not before.
- **A backup of the authored side.** See the risk below; wanted by 14 September.

## Deliberately out of scope

- **A second data source.** Withings body weight is the strongest candidate — the
  degenerate entity § II.3 names, and it would exercise § 6's comparability
  classes, which nothing has touched. The architecture claims source
  independence and has never been tested against a second source, so this gets
  more expensive the longer it waits. It competes for the same weeks and does not
  help the operator train.
- **The macro layer** — nutrition, the family calendar, and anything that
  *decides* how a week is spent. Slots are recorded **and allocated**; what waits
  is choosing the split.

## Risks

- **The store is the only copy of authored data, and that stops being cheap on
  14 September.** Raw landing re-fetches from Hevy and everything derived
  rebuilds; programmes and prescriptions do not. Today they are beta-testing
  artefacts and losing them costs a re-extract and some re-authoring — so do not
  be precious with the store, and migrate it or start fresh without ceremony.
  **Once the autumn block is running, the authored side is a primary input with
  no way back** (§ 12). A backup wants to exist by then.
- **`prescribing::deliver` hardcodes `catalogue::source("hevy")`.** The one place
  the tool is genuinely coupled to a vendor. It should be a `--to` argument or
  derived from the programme.
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
