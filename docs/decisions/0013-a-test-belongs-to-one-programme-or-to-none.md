# 0013 — A test belongs to one programme, or to none

**Date**: 2026-08-22
**Status**: Accepted
**Closes**: the open question in
`0011-the-test-is-for-the-load-the-progression-stands-at.md`.
**Amends**: `0009-a-linear-block-opens-from-its-entry-test.md`, whose recency
judgement becomes a rule. See its own amendment.
**Requires**: `0012-programmes-succeed-one-another.md`.

## What was decided

```text
linear   never includes a test.  May take a preceding one as input,
                                 otherwise it declares its opening.
block    always includes its exit test.  Requires a preceding one as input.
test     a programme in its own right, belonging to neither neighbour.
```

**A preceding test is usable as input when it is the same exercise, and it falls
in the week before the programme or the week before that.** Both conditions are
the operator's, stated on 2026-08-22.

## Why

**Because "may or may not end in a test" forked what `duration_weeks` means.**
Under the old model a linear block always tested and its duration silently
included that week, so `climbing_weeks = duration_weeks - 1`. Making the test
optional would have made `5` mean either five climbing weeks or four and a test,
distinguishable only by a flag beside it. Saying linear never tests removes the
fork instead of encoding it.

**And it makes the composition rule fall out rather than be stated.** The
operator's six examples reduce to two questions — does the successor require a
test, and is a usable one already there:

```text
                                            same lift   inherited   standalone
fs linear        →  fs block                 yes         no          required
fs block (exit)  →  fs linear                yes         yes         none
RDL linear       →  fs block                 no          no          required
RDL block (exit) →  fs linear                no          no          optional
RDL linear       →  fs linear                no          no          optional
RDL block (exit) →  fs block                 no          no          required
```

Row one needs a standalone test because linear never tests — not because of
anything about blocks. Rows three to six turn on the lift: a test in the wrong
lift is not a test, because there is no relationship between a front squat
maximum and an RDL one.

**Block requires `AnchorProvenance::Tested` specifically.** If an asserted anchor
satisfied a block's entry requirement, the RDL switch could skip the test by
asserting a number, and rows three and six say it cannot. Linear accepts any
provenance, or none. That makes provenance load-bearing rather than merely
descriptive, which is a better use of a field that until now only recorded.

## What it changes, and what it does not

**The summer programme is re-described and nothing it prescribes moves.**

```text
now      linear, duration_weeks = 6, week 6 = test on 18 September
becomes  linear, duration_weeks = 5, rungs 85 · 87.5 · 90 · 92.5 · 95
         standalone test, 18 September
         block, from 21 September, inheriting it — same lift, week before ✓
```

Same five rungs, same test on the same day. Only the bookkeeping differs, which
is the strongest evidence the rule is right: it re-describes what the operator is
already doing without moving a single load.

**`WeekKind::Test` stays.** It is what a block's last week is and what a
standalone test programme's only week is. What changes is that a linear
calendar no longer emits it.

## What it costs

**Recency has to bucket weeks, and the existing helper cannot.**

```rust
fn offset_of(start: Date, date: Date) -> i64 {
    i64::from((date - start).get_days()) / 7
}
```

Rust truncates toward zero, so a test on Friday 18 September against a Monday 21
September start is −3 days → `0`, reading as the same week as the start rather
than the week before; Friday 11 September is −10 days → `−1` where the rule wants
`−2`. Recency must bucket both dates to their Monday and difference those.
Reusing `offset_of` would pass a happy-path test and be wrong in the middle of
every week.

**A standalone test needs a target, and takes it from the programme before it.**
That is decision 0011's rule, now consumed by a different programme than the one
that computes it.

## Consequences

- `template` gains a third value, `test`: one week, no ladder, reusing the
  calendar, the weekday map, the fills and `WeekKind::Test`.
- A linear `Calendar` stops emitting `WeekKind::Test`, and `Ladder`'s
  `climbing_weeks = duration_weeks - 1` becomes `climbing_weeks = duration_weeks`.
- `Programme::new` gains the recency check for an inherited anchor, and blocks
  additionally require `provenance == Tested`.
- 0009's `EntryTestIsNotBeforeTheBlock` still holds and is now the weaker half of
  the rule: the test must precede the programme *and* be recent.
