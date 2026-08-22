# 0012 — Programmes succeed one another

**Date**: 2026-08-22
**Status**: Accepted
**Extends**: the supersession model shipped with
`specs/003-prescribed-workout-generation`, which is not wrong — it is one of two
relations, and it has been carrying both.
**Raised by**: the autumn block, which cannot follow the summer one.

## What was decided

**Two relations, and they are not the same.**

```text
supersession   the same programme, re-authored to correct it
               latest authored wins — what `authored_at` already does

succession     a different programme, later in time
               both are real; which one answers depends on the date asked about
```

**The programme that answers a date is the one whose window contains that date**,
not the most recently authored one. Windows may not overlap.

**A programme's identity is declared, not inferred.** The document names it:

```toml
[programme]
name     = "autumn-2026-front-squat"
template = "block"
```

A name the store has not seen is a new programme. A name it has is a modification
of that programme, and `authored_at` goes on doing supersession *within* it.
`programme author` reports which of the two it is doing rather than deciding
quietly.

## Why

**Because the store holds exactly one programme.** `ORDER BY authored_at DESC, id
DESC LIMIT 1` — the programme in force is the most recently authored, full stop.
Authoring the autumn block would not follow the summer linear; it would replace
it, and a date inside September would then be answered against the autumn
calendar or refused as out of range.

**And composition is the point.** The operator's six worked examples are all
succession:

```text
front squat linear  →  standalone test  →  front squat block
front squat block   →  front squat linear
RDL linear          →  standalone test  →  front squat block
```

A block has a minimum and a maximum number of weeks; a linear programme absorbs
whatever the calendar leaves at either end. None of that is expressible while one
programme is in force at a time.

**Identity cannot be inferred from the start date.** It was the obvious natural
key and it is wrong: correcting a start date would silently fork a new programme
rather than amend the one that exists. The operator's instruction was that
modifying and creating must be distinguishable, and an inference that is right
most of the time is exactly the kind that fails silently.

**The name belongs in the document rather than in a flag.** § 12 makes the
authored record a primary input that nothing regenerates — so the record has to
be reproducible from the document alone, and `--amend` is invocation state that
no document remembers. A typo cannot quietly create a phantom programme because
authoring prints what it is about to do, and a new name whose window overlaps an
existing programme is refused outright.

## What it costs

**A date can now belong to no programme, and that is a real state.** Between two
programmes, or before the first, nothing is planned. It has to read as "nothing
is planned for then" rather than as an error about the current block — the second
would be a lie about which programme was consulted.

**Overlap becomes an invariant.** Two programmes covering one day would make the
prescription for that day depend on how the query happened to be ordered, which
is the silent data loss § 12's discipline exists to prevent. Authoring refuses
it.

**Reading is no longer one row.** The read path stops being `LIMIT 1` and starts
being a lookup by date, and `programme show` has to say *which* programme it is
showing.

## Consequences

- `programme` gains a name, and the pair (name, authored_at) is what supersession
  is keyed on.
- Selection by date replaces selection by recency in `ProgrammeStore`.
- `prescribe` for a date in no window is a reportable outcome, not an error.
- The wizard has somewhere to put "is this a new block or a fix to the last one?",
  which is a question it could not previously ask.
