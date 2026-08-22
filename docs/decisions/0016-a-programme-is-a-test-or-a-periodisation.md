# 0016 — A programme is a test or a periodisation

**Date**: 2026-08-22
**Implements**: `0013-a-test-belongs-to-one-programme-or-to-none.md`, whose
consequences this settles in types.
**Amends**: `0014-block-periodisation-keeps-its-endpoint.md`, whose entry-test
week becomes optional here rather than mandatory. See its own amendment.

## What was decided

```text
Programme  ─┬─ Test                    one week, no ladder, a maximum
            └─ Periodisation ─┬─ Linear   a top-set ladder at a rate
                              └─ Block    phases to a planned endpoint
```

**Two levels, because there are two questions.** Whether this programme measures
or progresses is the first, and a test answers it by sharing none of what the
other two share — no anchor, no gating role, nothing that climbs. Only once the
answer is "progresses" does the second arise, and `block.rs` has called linear
and block two models of periodisation since 2026-08-18.

Flattening them into one enum would put `Linear` and `Test` side by side and lose
exactly that. `Periodisation` is the operator's word, chosen over `Progression`
because `progression.rs` already holds `Progress` — where the record stands — and
two near-identical names one module apart is a misreading waiting to happen.

Five things follow, each settled by the operator on 2026-08-22.

**1. A block may run its own entry test, as an optional week in front of its
phases.** This was first settled the other way — the entry test moved out of the
block entirely, as it had moved out of the linear template — and the operator
overturned it the same day, before any of it merged.

The reason is that a test session has to run the lower slots the way the
programme it anchors runs them: testing the front squat makes the hip-dominant
slot that day's accessory. So the pattern, the primary exercise and the accessory
fill have to agree between the test and the block. Three facts that must agree
across two documents, with nothing checking them, is worse than the duplication
it looks like — small enough to seem harmless, load-bearing enough that
disagreeing breaks the session.

`duration_weeks` still counts phase weeks and only phase weeks. The fork 0013
refused for the linear template came from the test being its *last* week, folded
into the count, so `5` meant either five climbing weeks or four and a test. A
week prepended and flagged forks nothing: the calendar carries one more when the
entry test is there, and the number the operator's table states never changes
meaning.

**2. A test carries the repetition count it is performed at, whichever kind of
test it is.** `block.rs` enters a block on a triple deliberately — a cold maximal
single measures technique as much as strength, and a peaked one is what the
realisation weeks prepare for — and exits on a single. `Block::entry_reps` is
gone: the count belongs to the test that establishes the number, and by the time
the phases read that number it is an `Anchor` and already a one-rep maximum. The
check that the maximum table can convert it followed the count.

**3. A block that does not test its own entry requires
`AnchorProvenance::Tested`; one that does may open from any provenance, and
linear accepts any.** This is what makes 0013's table a rule rather than advice.
Rows three and six say a lift change into a block needs a test, and if an
asserted anchor satisfied the entry requirement the operator could skip that test
by stating a number.

A block that runs its own entry test is that case answered rather than evaded:
its anchor is what the operator *expects* to lift, week one is where they find
out, and a result that differs is answered by re-authoring — which 0012 makes a
supersession rather than a second programme. Requiring `Tested` there would be
requiring a test before the test.

**4. A test's fills are resolved when the document is read.** A test document
names the lift being tested and any fill moving with it; every slot it omits
comes from the programme before it. That resolution happens at authoring, so what
is stored is a complete `SlotFills` exactly as a linear programme's is.

**5. What the other session of a test week runs depends on whose week it is.**
The heavy session is always the test. For a standalone test the light session is
the predecessor's, at the load its progression stands at. For a block's entry
test it is the block's own, at a load the block *states*: the lift's maximum is
what that week is about to measure, so there is nothing to take a share of, and a
load inherited from a programme that may have trained a different lift would be a
number fitted to the record with no decision behind it. Absent means the session
is not run.

**And a block's entry test needs no target at all.** The ramp builds toward the
block's own authored anchor, expressed at the test's repetition count through
`rep_max` — so a triple works up to the 3RM the operator expects rather than to a
one-rep maximum nobody is attempting. No other programme is consulted, which is
the difference between this and the standalone test: what a block expects is the
block's own statement.

## Why

**Because a test is not a degenerate progression.** The first cut proposed here
was one `Programme` struct with a `Plan` enum field, which would have made a test
a linear programme with the ladder removed. The operator's cut names what the
categories actually are, and the code got shorter for it: `Periodisation` has one
`anchor`, one `gating_role`, one `entry`, and `Programme` has neither.

**Because the operator will not author a whole programme for two sessions.**
That was the stated objection and it shaped 4 and 5 between them. A standalone
test document is eleven lines of settled values; everything else — seventeen slot
fills, the warm-up ramp, six load scales — is inherited.

**And the same objection, pushed, is why a block owns its entry test.** If the
test has to state the coming block's pattern, primary and accessory anyway, then
authoring the test is authoring part of the block — so it should be authored
once, in the block, where nothing can disagree with it.

**Resolved at authoring rather than at derivation**, so that re-authoring the
summer block in October cannot silently move what the September test prescribed.
That is § 14's own rule read the other way: what was generated is recorded
concretely in the authored record, so the thing that produced it needs no
history. It costs order-dependence — the predecessor must already be stored — and
that is the right cost, because the alternative is a stored programme that is
incomplete on its own.

**The target is the exception, and deliberately.** It stays live because 0011
made it a function of where the record stands: every rung that goes up raises it,
so a number resolved at authoring would be stale the first time a session went
up. The distinction is that fills are intent and the target is read off the
performed record — one is inherited from another document, the other from
history.

## What it costs

**`PrescribedWorkout` no longer records an anchor.** It records a `DerivedFrom`:
an anchor for a session issued from a programme that climbs, a target for one
issued from a standalone test. A sum rather than two nullable fields, because
exactly one is ever the answer and two `Option`s can be got wrong in both
directions. Migration 0016 says the same thing in a `CHECK`.

**Every conditional column is a `CHECK` on the template.** Dropping the `NOT
NULL`s would let a linear programme be stored with no anchor and fail at
rehydration rather than at insertion. A test has no anchor, no opening and no
gate, does have a repetition count, and is one week; the database refuses each
violation and so does the domain, because a rule only one side holds is a rule
that drifts.

**A field a template has no use for is refused rather than ignored.** A
`gating_role` on a test is the operator believing something untrue of that
programme, and reading past it silently is how a document and what it authors
come apart.

**`programme show` gains `--date`.** With one programme ever in force "the
programme" was unambiguous. With three in the store it is a question about a day.

## What each is for

```text
standalone test   between two linear programmes, or with nothing after it
block entry test  before a block, which is what a block is entered on
```

A standalone test remains useful and is not superseded, but it is no longer how a
block gets its anchor.

## What is not decided here

**The recency rule.** 0013 requires an inherited test to fall in the week before
a programme or the week before that, and records that the existing `offset_of`
helper cannot answer it — Rust truncates toward zero, so a Friday test against a
Monday start reads as the same week. Nothing here buckets weeks to their Monday,
and `Programme::new` does not yet check recency. What is enforced is 0009's
weaker half: the entry test must precede the programme.

**Deriving an anchor from a performed test.** A block's anchor is still authored,
with `provenance = "tested"` and the test's date. Reading the standalone test's
result out of the record and handing it to the next programme is the other half
of inheritance and is not built.
