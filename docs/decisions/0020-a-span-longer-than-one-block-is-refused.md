# 0020 — A span longer than one block is refused, not split

**Date**: 2026-08-27
**Raised by**: the operator, asking why the wizard would not let him pick the
periodisation, and tracing that back to a remark of his own about splitting a
period into more than one.
**Amends**: [0019](0019-the-wizard-asks-what-is-known-and-derives-the-rest.md),
which removed the arithmetic behind `how many weeks of phases?` and left the
question standing.

## What was decided

**Fifteen weeks of phases is the ceiling, and a span that will not fit one
block is refused rather than filled.** `Block::new(16)` is `TooLong` because
the top-set ladder would open above the maximum for its own repetition count —
a rule the domain has enforced since block periodisation shipped. What was
decided here is that the *wizard* says so plainly and stops, rather than
offering to put something in the remainder.

**So the end date is the only statement of length.** `how many weeks to
programme?` is gone. It let an answer contradict the `ends?` given one line
earlier, and the gap it opened had nothing to fill it. A block that should end
sooner is a block with an earlier `ends?`.

**And the refusal points the right way.** One line said `try a later end`
whichever way `Block::new` failed. For a block already too long that advice
makes the next attempt worse than the one it was correcting.

## What was considered and rejected

**Filling the remainder with a second periodisation.** A span of twenty weeks
would run a block for sixteen and a linear ladder for four, held as two
programmes in succession — decision 0012's existing relation, no new domain
type, non-overlapping windows.

It was rejected on the anchor. `Linear::new` takes an `Entry`, and an `Entry`
is a starting 1RM in kilograms with a provenance and a date. At authoring time
the only number available for the second programme is what the first *plans* to
finish at — 105% of its entry anchor, by `Block::ENDPOINT`. That is a number
asserted about a test that has not happened, written into an authored record
that § 12 says is a primary input with no way back.

The alternatives were worse:

- **Author only the first, and come back.** Honest, but then the span is not
  programmed and `prescribe` refuses every date past the first window until the
  operator returns. The wizard's whole purpose is to leave a block ready to run.
- **Give the second its own entry test.** No asserted number anywhere, but the
  first programme's exit test and the second's entry test are the same lift in
  adjacent weeks, and the span loses a week to measuring what it just measured.

**None of this is needed for a block anyone runs.** The autumn block is
thirteen phase weeks. A twenty-week span is a hypothetical, and the cost of
serving it was a persisted number that nothing measured.

## Consequences

- The wizard authors blocks between eight and fifteen phase weeks, and refuses
  outside that with the ceiling named.
- **A longer span is a second `programme add`**, authored when its predecessor's
  exit test is in and its anchor is a measurement. Succession already supports
  this; what is refused is authoring both at once.
- **If this is revisited**, the thing to change is the anchor, not the wizard.
  A programme able to say "my anchor is whatever the programme before me exits
  at" — resolved at prescription rather than at authoring — makes the split
  fall out. That is a real design and it is not a small one: it makes an
  authored record depend on a future measurement, which is the opposite of what
  § 12 and decision 0011 currently rely on.
