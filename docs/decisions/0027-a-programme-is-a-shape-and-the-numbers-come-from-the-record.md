# 0027 — A programme is a shape, and the numbers come from the record

**Date**: 2026-09-02

**Builds on**: `0026-do-not-mix-bounded-contexts.md`, which says what crosses
between programmes. This says what a programme holds at all.

**Amends**: `0009-a-linear-block-opens-from-its-entry-test.md`,
`0013-a-test-belongs-to-one-programme-or-to-none.md` and
`0016-a-programme-is-a-test-or-a-periodisation.md`, in each case only where they
put a measured number *on* the programme. Their reasoning about what opens a
block, what a test belongs to and what a programme is survives untouched.

**Generalises**: `0011-the-test-is-for-the-load-the-progression-stands-at.md`.
0011 is the model this decision applies everywhere.

**Dissolves**: open question 6 in `docs/roadmap.md` — *"what anchors a programme
that follows another?"* — which was a question only because anchors were
authored.

## Context

The operator, 2026-09-02:

> "I think the issue here is the difference between a programme shape, which the
> SBS chart gives us, and the prescription for a specific week in that
> programme, which can only be determined from performance. however, I don't
> think this is a problem, this is what they're supposed to be, the autumn front
> squat programme isn't supposed to say what the starting 1RM or ending 1RM is
> or should be."

And on the bike:

> "the FTP is the cycling equivalent of a 1RM, it comes from performance data.
> the entry test will set it for the sessions in the programme until we retest
> at the end."

And on the entry test:

> "the standalone entry test must inherit from performance data."

**This is not a new mechanism. It is two existing ones, generalised.** The code
already does this in two places and calls it something else each time:

- `TestTarget::Inherited` resolves a test's target *"from the programme before
  it, as the record stands"*, and 0011 gives the reason — every rung the
  predecessor climbs raises it, so a number written at authoring time is stale
  the first time a session goes up.
- SBS's `chart::maximum_after` reads each performed repetition-maximum day, runs
  it through the published table and applies the results in order, so week 3 is
  a share of what week 2 produced. A week nobody trained leaves the maximum
  where it was.

`Anchor` is the exception that was never brought into line. It is the same kind
of thing as both — a measurement — and it alone is authored into the programme
and frozen there.

## Decision

**1. A programme states shares, never magnitudes.** Sets, repetitions,
percentages, zones, durations, rest. No kilograms, no watts.

**2. The magnitude is read from the record when a session is prescribed**, not
when a programme is authored. For the gym that is the current maximum for the
lift; for the bike it is FTP. They are the same thing under two names, and the
operator said so.

**3. `Anchor` is not the wrong type — it is in the wrong place.** It carries a
load, an optionally failed load, a provenance and a date, which is precisely the
shape of a measurement. Measurements belong with performances. What replaces the
field is a question asked of the record: *what is the current maximum for this
lift, as at this date?*

**4. FTP is the cycling maximum, under the same rule.**
`domain::cycling::zone` already has this right and says so — FTP is *"an
interpretive parameter under § 13, so it is effect-dated and retained. The value
in force when a session was prescribed is the one that applies to it, and a
later test supersedes without rewriting anything already issued."* That sentence
is the whole of this decision, written for one discipline before it was a rule.

**5. Provenance survives, attached to the measurement rather than the
programme.** Tested, estimated and asserted are facts about how a number was
arrived at, and the difference *"matters six months later and is not recoverable
from the number"*. `AnchorProvenance` and `FtpProvenance` are the same enum
twice and stay that way — they belong to their own contexts (0026) and must not
be merged into one shared type.

**6. An asserted measurement needs a route in, and for cycling it is required
rather than optional.** The gym has Hevy, so a 1RM can be tested and landed. The
bike has nothing: Peloton is neither a source nor a sink, and the operator ruled
out the obvious shortcut on 2026-09-02 — *"they don't have an official API and I
can't share my credentials with you in this chat, we must build a dedicated
route for asserting power zones."* So an asserted FTP is not a fallback for the
bike, it is the only path until Peloton is a source.

## Why this does not violate § 12

The roadmap raised the objection and it was right at the time:

> "It is not a small change: it makes an authored record depend on a future
> measurement, which is the opposite of what § 12 and 0011 rely on."

That objection holds against an anchor as a **field**. § 12 protects primary
inputs — things that cannot be regenerated if lost — and an authored number is
one. It does not reach a **derivation**, because a derivation regenerates by
definition.

And nothing already issued moves, because § 12.1 already settled that: a
prescription is drafted, published or performed, and a published one is recorded
rather than re-derived. So a session issued against Tuesday's record stays as
issued when Friday's record changes. SBS has been relying on exactly this since
the chart landed.

## What this deletes

- `Entry`, and `declared_opening` with it. The either/or it existed to make safe
  — a declared opening meaning the anchor's failed load feeds nothing — has no
  cases left.
- `Anchor` as a field on `Linear`, `Periodised`, `Sbs` and `Test`.
- `TestTarget::Declared`. *"Stated, where there is nothing to inherit from"*
  becomes an asserted measurement in the record, which is the same escape hatch
  in the right place.
- The anchor columns on the programme tables, and the wizard questions that
  fill them.

Seventeen files reference `Anchor` or `Entry`. Almost all of it is removal.

## Consequences

- **The autumn block can be authored on 13 September.** Three consecutive SBS
  cycles needed two starting maxima that only the first cycle's test could
  produce; there is now nothing to supply.
- **The entry test week's non-test session reads the summer block.** The
  operator, on whether the autumn test inherits from `summer-2026-front-squat`:
  *"yes, but only from the perspective of assuming a 1RM for the non test
  session in the entry test week. summer has been too interrupted by holidays,
  that's why the autumn block needs an entry test."* So the inheritance is used
  for the session that is not the measurement, and the measurement measures.
- **A programme becomes re-runnable.** With no numbers in it, the same authored
  SBS cycle can be run again in January against January's record.

## Open

**Where an asserted measurement is entered, and under what command.** § 14 makes
it a generation parameter, and `fitness parameters` already reports the numbers
prescriptions are generated against but cannot set one. Whether asserting a
maximum is a `parameters` verb, a discipline verb, or part of the planner is not
settled here.
