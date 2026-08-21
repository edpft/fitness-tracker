# 0010 — An interruption is a run of days, not a week

**Date**: 2026-08-21
**Status**: Accepted
**Supersedes**: the week-named interruption introduced with the calendar in
`specs/003-prescribed-workout-generation`.
**Raised by**: the block starting 3 August, whose September the old model could
not express.

## What was decided

**An interruption names sessions.** It is a start date and a count of
consecutive days, at least one — `Skip { start, days }` — and the block does not
run on any day it covers.

**A week is interrupted only as a consequence.** It is a training week if at
least one of its sessions survives, so a week that loses everything is not a week
of the plan and pushes the block one calendar week further out, exactly as
before. A week that loses only some of its sessions costs the block nothing: the
ladder advances through it.

```text
away Monday, back for Friday    a training week, one session short
away all week                   not a training week, the block runs a week later
```

## Why

**Because the operator's September is four individual sessions and the week model
could not say so.** Monday 31 August, Friday 4 September, Friday 11 September,
Monday 14 September. Under the week model:

- naming the week of 7 September to lose its Friday also lost its usable Monday,
  which is a session that is going to be trained;
- and the week of 14 September could not be named at all, because its Friday is
  the block's test.

So the model was wrong in both directions at once — it removed a session that
runs and could not remove one that does not. Neither is a matter of degree: the
prescription the operator gets for 7 September is either issued or not.

**A count rather than an end date**, which is the operator's own design. There is
no `end` column and no `through` field to disagree with the start, so a backwards
range is unwritable rather than rejected, and there is no fallible constructor to
carry the rejection. `days` is a `NonZeroU8`, so an empty skip — one that would
author successfully and skip nothing — is unwritable too.

**One representation, not two.** `Skip::day(d)` is a one-day run rather than a
variant beside a range, because `Day(d)` and `Range { start: d, days: 1 }` would
be two spellings of one fact that compare unequal. The document may still write
either, and both normalise on the way in:

```toml
interruptions = ["2026-09-04", { start = "2026-12-22", days = 12 }]
```

## What it costs

**Calendar weeks are walked rather than counted.** Under the week model the
span was the duration plus the number of interruptions, which was addition.
Whether a week runs is now a question about the weekday map and the skips
together, so `Calendar` walks the weeks from the start until it has counted the
training weeks it needs. That is a loop where there was arithmetic, bounded by
the duration plus the furthest week any skip reaches into.

**A set of skips can now leave a block that never completes**, which the old
model could not express either. Skipping every Monday and every Friday for
longer than the block is a document that authors nothing, so
`InvalidCalendar::NeverCompletes` exists to say so at authoring time rather than
at generation time.

**Overlaps are permitted and mean nothing.** A day skipped twice is skipped. The
alternative — refusing an overlap — would reject a document that says something
perfectly clear, and there is nothing for the second skip to corrupt.

## Consequences

- `Skip` is a domain type: `start`, `days`, and a derived `last()`. The last day
  is never stored beside the first, because a second source of truth is a second
  thing that can disagree.
- `NotScheduled::Interrupted` names the whole skip rather than the date asked
  about: `2026-12-25 is not run: this block skips 2026-12-22 to 2027-01-02`.
  "The 25th is not run" is less use than "you are away the 22nd to the 2nd". A
  one-day skip displays as the bare date, so it does not repeat itself.
- `programme author` prints *sessions*, not weeks, and `programme show` reports
  the training weeks over the calendar weeks the walk found.
- Migration `0014` replaces `programme_interruption.week` with `start_date` and
  `days`, with `CHECK (days >= 1 AND days <= 255)`. Existing rows become
  Monday-to-Sunday seven-day ranges, which is a restatement rather than a guess:
  a week named under the old model did not run at all, so every day in it was
  skipped.
- The `Calendar` property tests skip whole weeks rather than single days, since
  the properties they hold are about how a block absorbs a week away — and the
  whole-week case is now one shape of skip rather than the only one.

## What was deliberately not decided

**Strength loss over a long layoff.** The question that prompted this decision
was "at what point do we assume strength has been lost", and three shapes were
on the table:

- nothing — an interruption of any length holds the ladder;
- a long interruption ends the block, so the next session is a test;
- a third reset protocol keyed on absence.

**None was built, and that is the decision.** Any threshold and any drop would be
numbers nobody can validate from the operator's record: published detraining data
is heterogeneous in training age, in the lift, and in whether anything was done
during the layoff, so a coefficient chosen here would look exactly as
authoritative as one fitted to the corpus and would be wrong in a way no test
could catch.

**The middle shape turned out to need no mechanism at all.** The operator's own
example — if Monday 7 September were not doable, the block should end with a test
on Friday 28 August — is `duration_weeks = 4`:

```text
w/c Aug  3   Fri  7   rung 1   85
w/c Aug 10   Fri 14   rung 2   87.5
w/c Aug 17   Fri 21   rung 3   90
w/c Aug 24   Fri 28   test  →  going for 92.5
```

The test target falls out of decision 0011 — every rung made, so one climb past
the last. So **"a long interruption ends the block" is an authoring decision, not
a derivation**: the operator knows their calendar when they author, and nothing
in the model needs to conclude that a gap was too long.

Revisit it when there is a real layoff to look at. That would be one data point,
which is one more than exists now.
