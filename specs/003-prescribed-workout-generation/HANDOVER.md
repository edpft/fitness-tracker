# Handover — prescribed workout generation

Written 2026-08-18, mid-feature, for whoever picks this up next.

**Branch**: `003-prescribed-workout-generation`, 16 commits ahead of `main`,
nothing pushed. 227 tests pass, `clippy --all-targets -- --deny warnings` is
clean, `nix flake check`'s `architecture` and `use-case-isolation` pass. 52 of
80 tasks in [tasks.md](./tasks.md).

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

**Nothing else.** Several plausible-looking extra parameters were proposed
during design and every one was rejected by the operator as a guess in disguise:

- a total gain / ladder span (`92.5% → 105%`)
- an "opening proximity" to the rep-max
- an RIR per phase
- an opening percentage of 1RM

Each renamed the previous one. **The percentages belong to the template, and the
template is code** (§ 9: deterministic derivation is code, operator override is
data). If you find yourself asking the operator for a number that determines the
loads, you have re-introduced the same mistake.

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
terminates at 95% as a fact rather than an ambition — which is precisely what
the linear model's invented 105% endpoint could never be.

**RIR is never an input to a derivation.** `primary-lift-progression.md` says so
explicitly: it is an observation, retained for a retrospective check. A design
that consults it contradicts the model of record.

### The one genuinely open question

**The percentage table for an accumulation-into-intensification block, and how
it scales when duration changes the rung count.**

This is a literature question, not a question for the operator. Prilepin's chart
and standard block-periodisation templates are the sources. Do *not* reconstruct
it from the operator's 2025 block — its opening load was, by the operator's own
account, a guess.

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

- **The calendar counts calendar weeks, not training weeks.** `Calendar::place`
  computes days-since-start ÷ 7, so a holiday week silently consumes a ladder
  position. The operator's current block has two holiday weeks in it, and
  working around them by hand is exactly what they want to stop doing. This is
  the constraint calendar, listed as "Not modelled here" in the prescribed model
  and now a requirement. **Fix this early — it is a correctness bug, not a
  feature.**
- **`PerformedWorkoutReader`** is declared and deliberately not implemented; it
  needs a whole `GymWorkout` rebuilt from five tables. Belongs with the round
  trip.
- **User story 2 (the zero-rep sentinel) is not done.** The operator's failed
  95kg of 2026-07-03 is still a `RefusalReason::ZeroReps`. `Performed<M>` exists
  and the store column exists; only the translator arm and the refusal removal
  remain.
- **User story 3 (stall, reset, re-climb) is not done.** It belongs to `v1`.
- **The round trip** (`project` / `satisfies`) is designed but unwritten.
- **Three parameters remain marked `INFERRED`** in
  `crates/infrastructure/tests/fixtures/programme.toml`: `light_of_heavy`, the
  per-role top-set repetitions, and the per-block accessory ranges. They were
  read off the record rather than stated. Under `v2` the first two may disappear
  entirely.

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

1. **Fix the calendar** to count training weeks, with a holiday list on the
   programme. Correctness bug, blocks everything else.
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
