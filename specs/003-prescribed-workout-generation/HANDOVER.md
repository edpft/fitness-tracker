# Handover — prescribed workout generation

Written 2026-08-18, mid-feature, for whoever picks this up next.

**Branch**: `003-prescribed-workout-generation`, 18 commits ahead of `main`,
nothing pushed. 239 tests pass, `clippy --all-targets -- --deny warnings` is
clean, and **the whole of `nix flake check` passes** — `typos` and `toml-fmt`
were red when this note was first written and are fixed. 52 of 80 tasks in
[tasks.md](./tasks.md), plus the calendar fix, which is not one of them.

Read [spec.md](./spec.md), [plan.md](./plan.md) and [research.md](./research.md)
before writing code. This note covers what those cannot: what changed *during*
the work, and what it cost to find out.

---

## The one thing to understand first

**The design changed fundamentally near the end, and the code has not caught
up.**

What is built is a *linear* progression for the primary lift: a percentage
ladder climbing to an authored endpoint, with a drop-and-re-climb on failure. It
works end to end — `fitness prescribe` issues a complete, trainable session from
the operator's real store.

What the operator actually wants, and what was settled in conversation, is
**two** programme types selected by how many weeks the calendar allows:

| Window | Programme | State |
| --- | --- | --- |
| 7+ weeks | Block periodisation: entry test → accumulation → intensification | **not built** |
| short, interrupted | Linear top-set/back-off, +2.5kg, reset protocol | built, works |

`Programme` is already `enum { V1(..), V2(..) }` with append-only variants,
which is exactly this shape. **The linear work becomes `v1`. Periodisation is a
new `v2`.** Do not delete or rewrite the linear model — it is the right tool for
the pre-Christmas window and the operator has said so.

---

## The `v2` design, as settled

### Inputs — and there are only three

```text
duration      weeks available, from the family calendar
target RM     what the block is for, e.g. a 3RM
entry test    the anchor, tested before the block begins
```

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
week 1        entry test at the target RM
weeks 2..     accumulation    — many sets, reps descending, load rising
   ..N        intensification — one set, reps descending, load rising
week N        the last intensification week IS the exit test (not a separate week)
```

- Phase split: the entry test takes one week; the remainder splits 50/50 with
  **intensification dropping first**. Minimum 3 of each, so **minimum block is 7
  weeks** (1 + 3 + 3).
- The rep ladder is generated **backwards from the target RM**. A 3RM target
  with 5 intensification weeks gives 7, 6, 5, 4, 3. Duration sets the rung
  count; this is what makes the block adapt to whatever the calendar gives.
- The second phase restarts at *higher* reps and *lower* load than the first
  ended. That wave is the design, not an artefact.

Derived split, verified against the operator's stated rule:

```text
block  phase wks  accum  intens
    7          6      3       3
    8          7      4       3
    9          8      4       4
   10          9      5       4
   11         10      5       5
```

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

**RIR is never an input to a derivation.** `primary-lift-progression.md` says so
explicitly: it is an observation, retained for a retrospective check. A design
that consults it contradicts the model of record.

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
volume from intensity inside the week because it has no blocks.** `v2` has
blocks. Splitting again inside the week would do the same job twice, and the
block-periodisation sources say the opposite — both sessions carry the block's
character, differentiated by load or by variation. So the second session is the
same week's rung, lighter, and it costs no new parameter.

**Settled 2026-08-18: both sessions run the front squat, and the lighter one is
85% of the week's load.** Not a variation on the light day — the operator was
offered one and declined it. `light_of_heavy` carries into `v2` unchanged in
meaning; only the number moved, and why it moved is the next section.

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
  outlives only `v1`.
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
convention puts a PR attempt at 102–107% of the previous best. In our terms,
where the exit is a rep max rather than a single, `105% × rm(target)` — for a
3RM, about **100% of the entry 1RM**, so a 3RM block plans to exit with a triple
at the entry test's one-rep max.

**`v1`'s 105% was never invented.** This note called it "the linear model's
invented 105% endpoint". It is the standard figure, discarded because nobody
could justify it, at which point nobody looked.

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
w/c 14 Sep     autumn block begins → Sun 29 Nov = 11 weeks → 5 accumulation / 5 intensification
Mon 30 Nov     mini-cut to Christmas — v1 linear territory, too short for a block
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
- **User story 2 (the zero-rep sentinel) is not done.** The operator's failed
  95kg of 2026-07-03 is still a `RefusalReason::ZeroReps`. `Performed<M>` exists
  and the store column exists; only the translator arm and the refusal removal
  remain.
- **User story 3 (stall, reset, re-climb) is not done.** It belongs to `v1`.
- **The round trip** (`project` / `satisfies`) is designed but unwritten.
- **Two parameters remain marked `INFERRED`** in
  `crates/infrastructure/tests/fixtures/programme.toml`: the per-role top-set
  repetitions and the per-block accessory ranges. They were read off the record
  rather than stated, and the third one — `light_of_heavy` — turned out to be
  wrong when the operator looked at it. Under `v2` the top-set repetitions may
  disappear entirely.
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
2. **Research the percentage table** for accumulation/intensification. The last
   unknown, and it is a literature question.
3. **Build `v2`** beside `v1`. `Ladder` becomes a table lookup; the anchor,
   slots, template, fills, quantisation, all four stores, the document reader,
   the CLI and every accessory scheme are untouched.
4. **User story 2**, so a failed attempt stops being a refusal.
5. The rest: stall/reset for `v1`, the round trip, the decision records (`0006`,
   `0007`) named in [plan.md](./plan.md).

The operator's deadline is **Sunday 13 September**. Steps 1 to 3 are what has to
land by then.
