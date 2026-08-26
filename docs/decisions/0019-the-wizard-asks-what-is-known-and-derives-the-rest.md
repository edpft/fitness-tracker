# 0019 — The wizard asks what is known and derives the rest

**Date**: 2026-08-26
**Raised by**: the operator, reviewing a full run of `fitness programme add`.
**Scope**: what the authoring questions ask. Not what a programme *is* — every
type below already existed, and three of them were being asked about in units
nobody thinks in.

## What was decided

**A question is worth asking only if the operator is the one who knows the
answer.** Three questions failed that and are replaced.

### 1. A block is stated by its dates

`how many weeks of phases?` asked for a count of accumulation, intensification
and realisation weeks, excluding an entry test that the wizard then added back.
Unanswerable without reading `block.rs`: the number that comes out is not the
number of weeks the block occupies, and the word "phases" appears nowhere the
operator would look.

So it asks `starts?` and `ends?`, and derives. The operator states a block the
way he decides one — "from the week commencing 14 September through the week
commencing 14 December" — and how that divides into a test week and three
phases is the tool's business.

**The dates hold and the phase count absorbs what the schedule takes.** A week
the diary leaves nothing in is not a training week, so a span with a holiday in
it holds one fewer. This is not a new rule: `Calendar` already counts a week as
a training week if at least one of its sessions survives (2026-08-21), and
`Calendar::training_weeks_within` is the same walk run the other way. The two
directions share one `week_runs`, so a duration derived from two dates spans
back to exactly those two dates — which is the property the tests assert, with
and without a week lost.

**The count is still asked, with the span as its default.** It earns its place
in two cases: stopping short of the span, and a span holding more phase weeks
than one block can — where the remainder has to go somewhere, and deciding that
is the platform-level feature this defers.

**Fifteen phase weeks is the ceiling and it is derived, not authored.** The
operator believed the model stopped at twelve, and would have had to programme a
linear tail onto the autumn block to fill fourteen weeks. It does not: the 8 to
11 table is four rows of a rule that carries on, and what bounds a block is that
the top-set ladder opens at a repetition count equal to the two phases it spans
— past nine reps at 80% that is an impossible set rather than a hard one. The
autumn block is thirteen phase weeks and plans as 5-5-3, in one block.

### 2. The primary is knee or hip dominant

Not upper push, not upper pull. The ladder, the anchor and the entry test are
all about a lower-body maximum, and an upper pattern is an accessory slot
whichever block it sits in. The four-item list came from treating `SlotId`'s
patterns as interchangeable; the operator has now said twice that they are not.

### 3. The entry test is matched, beaten, or declared

`expected one-rep maximum?` followed by `as of which date?` mixed two things
that are not the same. A number plucked out of the air has no date — there is
nothing for one to mean — and a number pointing at a performance has that
performance's date, which is not the operator's to type. The real question is
what a reasonable target is.

So the record answers first and the operator states an intent:

```text
your best front-squat is 90kg × 3, on 2025-04-28 — a maximum of 95kg
 1. match it                              95kg
 2. beat it                             97.5kg
 3. a number of my own
```

"Beat it" is one step on that exercise's own scale, so it lands on the plate
grid rather than being typed. The date follows from the choice rather than being
asked: the performance's for 1 and 2, the block's start for 3.

**Never `tested`.** The record shows a set, not a test — a completed single may
have been a top set rather than an attempt at a ceiling — so reading a maximum
off it is `estimated` however few repetitions it took. Only a test this tool
issued may claim to have tested anything. A beaten maximum is `asserted`,
because nobody has lifted it.

**The best is all-time, and that is a deliberate for-now.** The operator settled
it on 2026-08-26.

## Why all-time, when what was asked for was *recent*

**Because the two better answers each need something that does not exist yet.**

**Not "where the preceding programme's progression stands"**, which was the
proposal and is wrong for a reason worth recording: *there is no guarantee the
lift being planned was trained in the programme before it*. A front squat block
may follow a deadlift block, and `Progress::test_target` would then answer about
a different lift or not at all. Decision 0011's rule is about a test *inside* a
programme, where the lift is the programme's own; an entry test looking backward
across a boundary is not that question.

**Not "the best within the current macrocycle"**, which is the answer the
operator wants and which has the same hole from the other side: the first block
to train a lift in a macrocycle has nothing to look at. A macrocycle is also not
modelled — programming is deliberately mesocycle-level today.

**Not a window of N weeks**, because N is a number nobody has stated, and a
number chosen here to make an example come out right is the fault
`light_of_heavy` was caught with.

So all-time, which is honest about being a floor rather than a judgement: it
shows the set and its date, and the operator can see for himself that a maximum
from sixteen months ago is not evidence about September and pick option 3.

## What it costs

**A stale maximum is offered as the default.** On the operator's own store the
best front squat is a triple from April 2025, implying 95kg, while the block
running now prescribes 92.5 — so "beat it" proposes 97.5 off a set that predates
a year of training. The guard is that the set and its date are printed, not just
the figure. That guard is weaker than a rule and is why this is a for-now.

**It gets worse as the record grows**, not better: an all-time best is a ratchet,
so a bad year leaves the proposal pinned to a good one. The fix is the
macrocycle window, and this decision should be amended rather than quietly
outgrown.

## Consequences

- `Calendar::training_weeks_within` is new and is the only new domain surface
  here. Everything else is the wizard asking differently.
- `PATTERNS` in `cli::wizard` holds two entries.
- `Draft` carries a provenance, so the document no longer states `asserted`
  unconditionally.
- `docs/roadmap.md` step 3 records what changed about the wizard it describes.
