# Handover — prescribed workout generation

Written 2026-08-18, mid-feature. **Updated 2026-08-19**, and the numbers below
are the current ones.

**Branch**: `003-prescribed-workout-generation`, 35 commits ahead of `main`,
nothing pushed. 285 tests pass and **the whole of `nix flake check` passes** — all
18 checks. **All 80 tasks** in [tasks.md](./tasks.md) are done. The feature is
complete; the next thing is wiring `block` above `domain`, which is a feature of
its own.

**Read D13 and D14 in [research.md](./research.md), and decisions `0008` and
`0009`, before anything else.** The linear ladder no longer has an endpoint *or*
an authored opening: it climbs at a stated rate from wherever the entry test puts
it. That contradicts several passages further down this file, written before
either was settled. Where they conflict, D13 and D14 win. The passages are left
standing because *why* they were wrong is the most useful thing in this
document.

Read [spec.md](./spec.md), [plan.md](./plan.md) and [research.md](./research.md)
before writing code. This note covers what those cannot: what changed *during*
the work, and what it cost to find out.

---

## The one thing to understand first

**The design changed fundamentally near the end, and the code has since caught
up.**

What is built is a *linear* progression for the primary lift: a ladder opening at
a percentage of the anchor and climbing 2.5kg a week, with a drop-and-re-climb on
failure. It works end to end — `fitness prescribe` issues a complete, trainable
session from the operator's real store.

**Note the row below says "+2.5kg" and has since 2026-08-18.** The code said
"climbing to an authored endpoint" for another day, because nobody read the two
side by side. If a table in this file and the code disagree, that is a finding
rather than a typo — see D13.

What the operator actually wants, and what was settled in conversation, is
**two** programme types selected by how many weeks the calendar allows:

| Window | Programme | State |
| --- | --- | --- |
| 9+ weeks (a test week and 8 of phases) | Block periodisation: entry test → accumulation → intensification → realisation | **built in `domain`** |
| short, interrupted | Linear top-set/back-off, +2.5kg, reset protocol | built, works |

**The linear work becomes the `linear` template. Periodisation is a new `block`
template beside it.** Do not delete or rewrite the linear model — it is the
right tool for the pre-Christmas window and the operator has said so.

They were `v1` and `v2` until 2026-08-18. Renamed because they are two models of
periodisation rather than two versions of one programme, and because `v1`/`v2`
collided with the operator's own programme document versions, which really are
v1 and v2.1. **`Programme` is not the `enum { V1, V2 }` this note used to
claim** — that was never built. `Programme` is one struct in `linear`, and the
store's `template` column is the discriminator.

---

## The `block` design, as settled

### Inputs — and there are only three

```text
duration      weeks of phases available, from the family calendar
entry reps    the repetition count the entry test is performed at, e.g. 3
entry test    the anchor, tested the week before the block begins
```

**The second input changed meaning in D12** and there are still three of them.
It used to be "the repetition maximum the block is for"; the block now finishes
on a single whatever it opened on, so what this says is what is being *measured*
at the start. Nothing else in the block reads it.

**Nothing else *from the operator*.** Several plausible-looking extra parameters
were proposed during design and every one was rejected — but read the next
paragraph before drawing the conclusion this list originally invited. What was
rejected was **the operator** supplying these numbers, not the numbers existing:

- a total gain / ladder span (`92.5% → 105%`)
- an "opening proximity" to the rep-max
- an RIR per phase
- an opening percentage of 1RM

Each renamed the previous one. **The percentages belong to the template, and the
template is code** (§ 9: deterministic derivation is code, operator override is
data). If you find yourself asking the operator for a number that determines the
loads, you have re-introduced the same mistake.

**And if you find yourself refusing a number because the operator declined to
invent it, you have made the opposite one, which cost longer.** The literature
supplies every one of the values in that list; D11 fills them in. The rule is
about where a number comes from, not about whether the block is allowed to have
any.

### Structure

```text
week 1        entry test, at its own repetition count
weeks 2..     accumulation    — many sets, reps descending, load rising
   ..         intensification — one set,  reps descending, load rising
   ..N        realisation     — one set,  descending to a single
week N        the last realisation week IS the exit test, and it is a 1RM
```

**Superseded by D12, in three places.** There are three phases rather than two;
the split is stated by the operator's research rather than divided 50/50; and
the duration counts *phase* weeks, with the entry test the week before them.

- The rep ladder is generated **backwards from a single**, and runs unbroken
  through intensification and realisation. Duration sets the rung count; this is
  what makes the block adapt to whatever the calendar gives.
- The second phase restarts at *higher* reps and the *same* load the first
  finished on. That wave is the design; the drop in the 2025 record is an
  artefact (D11 correction 1).

The split, as the operator's research states it and D12 records it:

```text
phase wks  accum  intens  realis
        8      3       3       2
        9      4       3       2
       10      4       4       2
       11      4       4       3
       12      5       4       3
```

Each week beyond the eighth goes to accumulation, then intensification, then
realisation, in rotation, so a duration nobody tabulated still plans. **The
maximum is fifteen weeks and it is derived, not authored** — beyond that the top
set opens above the maximum for its own repetition count.

### Loads

The RPE/RIR table published by Reactive Training Systems reduces exactly to:

```text
%1RM = 100 − 2.5 × (reps − 1) − 5 × RIR
```

Its role is **not** to prescribe. It fixes the ramp's *endpoint*: a 3RM test is
three reps at RIR 0, which the table puts at 95% of 1RM. So a 3RM block's ramp
terminates at 95% as a fact rather than an ambition.

That reading was too narrow. It fixes the relationship between a repetition
count and a load at one moment; it says nothing about how much the block gains,
and D11 correction 3 supplies that from the literature. The same passage
dismissed "the linear model's invented 105% endpoint" — 105% is the standard
figure in every peaking programme consulted, and was not invented.

**RIR is never an input to a derivation, and D12 goes further: there is no RIR
in the primary lift's block at all.** `primary-lift-progression.md` says it is
an observation, retained for a retrospective check. D11 argued that a *planning*
constant of three repetitions in reserve was a different thing; the operator
rejected that on 2026-08-18, and the argument was wrong — a percentage-based
plan states percentages, and `5 × RIR` is a coefficient of the RTS grid rather
than anything Prilepin published. Prilepin's repetitions-per-set column places
accumulation instead. See D12, correction 4.

### Sessions within the week

**One primary lift at a time.** The operator settled this on 2026-08-18: not two
lower-body lifts progressing in parallel. That matters, because their own
periodised block did the opposite and the corpus shows it plainly — front squat
on Mondays and only Mondays, deadlift on Wednesdays, back squat on Fridays,
three ladders running concurrently and phase-shifted against each other (the
deadlift was at its week 8 while the front squat was at its week 7). **That
pattern is not the model to rebuild.** It also means the record says *nothing*
about a second weekly session of one lift: there has never been one during a
block.

**A rung belongs to a lift and a week, not to a session.** Two sessions cannot
each take a rung — an 11-week block would need twenty-odd of them, which
contradicts duration setting the rung count. So the ladder hangs off the
primary, and the second session is derived from the same week's rung.

The literature is consistent about *how*: two sessions of one lift are
differentiated by role, never repeated. Starr's light day is 70–80% of the heavy
day's top set; the Texas Method's volume day is ~85–90% of its intensity day;
DUP runs ~70% against ~80% in one week. All three cluster on **the lighter
session being 70–90% of the heavier one**.

One structural point, because it decides the shape: **the Texas Method splits
volume from intensity inside the week because it has no blocks.** `block` has
blocks. Splitting again inside the week would do the same job twice, and the
block-periodisation sources say the opposite — both sessions carry the block's
character, differentiated by load or by variation. So the second session is the
same week's rung, lighter, and it costs no new parameter.

**Settled 2026-08-18: both sessions run the front squat, and the lighter one is
85% of the week's load.** Not a variation on the light day — the operator was
offered one and declined it. `light_of_heavy` carries into `block` unchanged in
meaning; only the number moved, and why it moved is the next section.

**Re-asked and re-settled the same day.** The operator asked whether the two
days should chase a 3RM on one and a 1RM on the other instead. No: within this
model a 1RM day measures nothing a 3RM day does not already imply, because every
load is derived through `rm(reps)` in which a 3RM *is* 95% of a 1RM. Two ladders
both ending near-maximal would also double the top-end exposure on one lift,
with RIR deliberately unavailable to absorb a bad week. **The single belongs in
realisation**, which is where the peaking literature puts it and which now
exists as a phase. See D12.

### The `INFERRED` parameter that was wrong, and how

`light_of_heavy` was 88.5%, solved from the record's three validated weeks:
72.5 / 75 / 77.5 light against 82.5 / 85 / 87.5 heavy.

**Every one of those pairs is a flat −10kg.** The percentage was a ratio fitted
to an offset. It reproduces all three only because quantisation rounds it back
onto the plate grid, and it drifts across them — 87.9%, 88.2%, 88.6% — where the
offset does not drift at all. The operator spotted it in one line: "I think it's
really 10kg pretending to be a percentage."

The note in `research.md` had even recorded that the offset "fits those three
equally well" and preferred the percentage for portability. That was the right
*shape* — an offset is a far larger relative drop at a 60kg anchor than at a
90kg one — and it did not license solving for the number inside it. **This is
the trap in this file's method section, caught in the act.** Two `INFERRED`
values are left; treat both as suspect in the same way.

### The percentage table — answered, in research D11

**Prilepin's chart supplies it, and nothing here is authored.**
Intensification's set is a rep max, so its load is the table D10 adopted read at
RIR 0. Accumulation cannot be a rep max, and Prilepin's admissible total lifts
per intensity band fix how far below it sits: three repetitions in reserve lands
every rung inside its band and the three-rep rung on the chart's optimum.

Three consequences, all in [research.md](./research.md) D11:

- **`ladder_start` and `ladder_end` stop being authored.** The span is derived
  at both ends, so the `TODO` that has blocked T080 since the feature began
  outlives only the linear template.
- **The wave is not a drop in load.** Intensification opens at the load
  accumulation left off at, with repetitions jumping and sets collapsing. The
  drop in the 2025 record comes from an accumulation ramp that started too
  light, not from a design that meant to overshoot.
- **The 8-week case reproduces the 2025 block's sets and repetitions exactly** —
  5×5, 5×4, 5×3, 5×2, then 1×5, 1×4, 1×3 — from the phase split and the target,
  compared with the record only afterwards.

### The block plans a gain, and the literature says how much — D11 correction 3

**This one went round in circles twice and the operator stopped it.** The first
attempt prescribed intensification against the entry 1RM, arriving at exactly
what was tested at the start. The second concluded that planning a gain needs a
number nobody could honestly supply, and proposed deriving each week from
performed top sets instead.

Both were the same mistake: **refusing a number the literature supplies, because
the operator had declined to invent one.** What was rejected earlier was the
operator picking a percentage out of the air. That is not the same thing as
consulting a published programme, and treating it as the same thing cost three
rounds.

**There are three sources of expertise here — the published literature, the
performed record, and what the operator states in conversation — and a rule that
bars one of them is a bug in the method.** The record is a diagnostic and the
operator will not guess; that leaves the literature carrying the numbers, which
is exactly what it is for.

**The endpoint is 105% of the entry 1RM.** The Russian Squat Routine ends with a
single at 105% of the starting max; Arbic's 17-week block programme tests at
105% of the *original* 1RM and is built so the lifter can double it; meet
convention puts a PR attempt at 102–107% of the previous best. **D12 simplified
the arithmetic**: the exit test is a single, so the endpoint is 105% of the
entry 1RM flat rather than `105% × rm(target)`, and the block plans a 5% gain
measured in the unit it was planned in.

**The linear ladder's 105% was never invented *by us*.** This note called it
"the linear model's invented 105% endpoint" and discarded it, at which point
nobody looked it up. It is the standard figure — and, as the operator observed,
suspiciously round in every source, which makes it a shared convention rather
than a finding. That is still worth more than a private invention: it makes the
block comparable with published ones, and it is falsifiable against the
operator's own exit tests after two or three blocks. **Revising it against those
results would be legitimate; fitting it to the record now would not**, and
`light_of_heavy` is the cautionary tale for the difference.

The intensification ladder therefore spans accumulation's exit to that endpoint,
and the implied 1RM climbs past 100% on the way — 97.1, 99.2, 101.2, 103.2,
105.0 over five rungs. **That is the operator's stated intent arriving as
arithmetic**: the intensification weeks land above what the entry test predicts,
and the exit test confirms the gain rather than discovering it.

**No load is prescribed for a test week**, which `WeekKind::Test` already says
by carrying no percentage. The endpoint is the warm-up ramp's target.

**What is left authored**: duration, the target repetition count, and the entry
test. Every load comes from those three plus three literature constants — the
repetitions-in-reserve table, Prilepin's bands, and the 105% endpoint. D8
closes.
---

## Evidence: the operator's own block, recovered from the corpus

March–April 2025, front squat. This is the shape being rebuilt.

```text
week  sets  reps  load   tonnage
   1     5     5    60      1500   ┐ accumulation
   2     5     4    65      1300   │ reps ↓, load ↑, tonnage ↓
   3     5     3  67.5      1012   │
   4     5     2    80       800   ┘
   5     1     5  77.5       388   ┐ intensification — sets collapse 5 → 1
   6     1     4    80       320   │ second wave: higher reps, lower load
   8     1     3    90       270   ┘ ← 3RM test, RIR 0 (week 7 missed)
```

Back squat and deadlift show the same shape in the same period, and **both
record an entry test** (deadlift `3×3@125` then `2×1@145`; back squat `4×3@85` …
`5×1@95`, exiting at `1×1@110`). Accessories in the same period look completely
different — `leg-extension 3×12` throughout with load climbing — which
independently confirms the primary/accessory split the model draws.

---

## Calendar and deadline

```text
Fri 28 Aug     last session of the current block (1×92.5 planned)
w/c 31 Aug     holiday
Sun 13 Sep     deficit ends  ← REAL DEADLINE for a plannable programme
Fri 11–Mon 14  holiday
Fri 18 Sep     3RM front squat test — the autumn block's anchor
w/c 21 Sep     autumn block begins → Sun 29 Nov = 10 phase weeks (4-4-2)
Mon 30 Nov     mini-cut to Christmas — linear territory, too short for a block
New Year       next proper block
```

**Anchors are per primary lift and do not carry across blocks.** The operator
alternates front squat with RDLs and possibly Bulgarian split squats, so a front
squat block's exit test anchors nothing but the next front squat block. An
earlier claim in this repo that "the previous block's exit test is the next
block's entry anchor" was wrong and has been corrected.

---

## Known gaps in what *is* built

- ~~**The calendar counts calendar weeks, not training weeks.**~~ **Fixed**
  (`fix: count training weeks, not calendar weeks`). A block's duration counts
  training weeks; the weeks it skips are authored per block, named by a date
  inside them, and stored in `programme_interruption` (migration `0005`). A date
  in a skipped week is refused rather than answered with a neighbour's loading,
  and `--date` defaults to the next *session* rather than the next programmed
  weekday. The programme store now takes the operator's zone, so `today` no
  longer resolves in UTC.
- **`PerformedWorkoutReader`** is declared and deliberately not implemented; it
  needs a whole `GymWorkout` rebuilt from five tables. Belongs with the round
  trip.
- ~~**User story 2 (the zero-rep sentinel) is not done.**~~ **Done** — a failed
  attempt is an outcome, not a refusal; decision `0007` records the reversal.
- ~~**User story 3 (stall, reset, re-climb) is not done.**~~ **Done** — drop back
  and re-climb when the plan was too ambitious.
- ~~**The round trip** (`project` / `satisfies`) is designed but unwritten.~~
  **Done**, as attribution rather than reproduction (T058, SC-010c).
- ~~**The ladder's opening is the last unauthored value.**~~ **Gone, not
  answered.** Both of D8's `TODO`s were dissolved on 2026-08-19: the endpoint
  stopped existing (D13) and the opening became a derivation from the entry test
  (D14). The fixture carries no `TODO`, `fitness prescribe` issues from it, and
  **all 80 tasks are done**.
- **The derived plan is more ambitious than what the operator runs.** From the
  corpus's entry test — 90 completed, 95 failed — an 8-week block issues one
  increment a week from 90 to 105: 90, 92.5 climbing in, then 95 … 105, then the
  test. The record's own July–August block ran 82.5, 85, 87.5 with 92.5 planned
  for 28 August. Attributable to a template change (SC-002) and defensible — the
  reset protocol is what finds the ceiling — but decision `0009` lists what would
  soften it further.
- **Two parameters remain marked `INFERRED`** in
  `crates/infrastructure/tests/fixtures/programme.toml`: the per-role top-set
  repetitions and the per-block accessory ranges. They were read off the record
  rather than stated, and the third one — `light_of_heavy` — turned out to be
  wrong when the operator looked at it. Under `block` the top-set repetitions
  may disappear entirely.
- **The hip-dominant slot alternates because there is no single hinge
  accessory.** Stated by the operator on 2026-08-18: the pattern splits into
  hamstring-focused and lower-back-focused work and one exercise does not cover
  both. It is a fact about the vocabulary rather than a loading device, so
  nothing about periodisation follows from it — and the primary slot does not
  alternate for the same reason inverted.

---

## Traps, each of which cost time here

**On method**

- **Do not fit parameters from the performed record and call the result
  evidence.** This happened repeatedly. The back-off percentage was only correct
  because the operator stated it; fitting it the way other values were fitted
  would have produced "the back-off holds while the top set moves" — a hand
  arithmetic error encoded as a rule.
- **The corpus is a diagnostic, not a specification.** It records a hand-run
  programme whose template changed while it ran and whose arithmetic was
  sometimes wrong. Success criteria assert *attribution* — every divergence
  falls into a named bucket — not reproduction. See SC-002, SC-003, SC-012.
- **A repeated exclusion is evidence the criterion is the wrong shape.** SC-003
  once excluded three sessions by date; that should have been read as the shape
  being wrong rather than the cutoff.

**On the toolchain**

- **Check `_sqlx_migrations` before modifying any migration.** A
  modified-after-applied migration broke the operator's store at the start of
  this work. `0004` was extended twice on the grounds that it had never been
  applied — true both times, but verified only the second time. **Any further
  change to `0004` should be an `0005`.**
- **`cargo sqlx prepare --workspace` silently skips test targets.** It needs `--
  --all-targets`, or the build breaks offline with confusing errors. Also:
  running it against a broken tree leaves the cache incomplete and produces a
  second, misleading round of failures.
- **Tests passing is not the gate passing.** `clippy --all-targets -- --deny
  warnings` is what CI runs, and it rejects pedantic casts, over-long functions
  (100 lines) and `panic!` in free functions.
- **`panic` is `forbid`, and the test exemptions do not reach free functions.**
  proptest strategy helpers must use `prop_filter_map` with `.ok()`, never
  `panic!`. `#[tokio::test]` and clap's derive macros are unusable for the same
  reason.
- **Scripted multi-file edits mangled files three times here** — a replacement
  hit the wrong occurrence and spliced a match arm into a function body; a TOML
  key landed inside the wrong table. Read the region back before running the
  next command, not after.
- **SQLite WAL sidecars persist.** `rm db` without `rm db-wal db-shm` leaves
  stale migration state and produces baffling errors.

**On working with this operator**

- Ask real questions or none. Do not write "I need one thing from you" and then
  not ask it.
- Do not narrate process problems they cannot act on.
- Long summaries with rhetorical questions are not wanted. They have not read
  the code; explain findings, not your own workflow.
- When they say a value is an example, it is an example. Do not derive structure
  from it.

---

## Suggested order

1. ~~**Fix the calendar**~~ — done. See "Known gaps".
2. ~~**Research the percentage table**~~ — done, D11 and D12.
3. ~~**User story 2**, user story 3, the round trip, the decision records~~ —
   all done. `0006`, `0007` and `0008` are written.
4. ~~**Ask the operator where the ladder opens**~~ — asked, and the answer was
   that it is not authored at all. See D14. T080 is closed and so is the feature.
5. **Wire `block` above `domain`.** `domain::prescription::block::Block` is built
   and tested (research D11, D12) and nothing above it selects it: no
   `application` use case, no CLI, no store. The anchor conversion (a 3RM entry
   test into the 1RM every percentage is a share of), the phase-aware second
   session and the calendar's extra test week are all still to come. **This is
   the autumn block's dependency**, per the calendar above, and it is a feature
   of its own rather than a task in this one.

The operator's deadline is **Sunday 13 September**. Step 5 is what the autumn
block needs, and it is now the only thing outstanding.
