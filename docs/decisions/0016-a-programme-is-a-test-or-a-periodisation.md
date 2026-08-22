# 0016 — A programme is a test or a periodisation

**Date**: 2026-08-22
**Implements**: `0013-a-test-belongs-to-one-programme-or-to-none.md`, whose
consequences this settles in types.
**Amends**: `0014-block-periodisation-keeps-its-endpoint.md`, whose entry-test
week is removed here. See its own amendment.

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

**1. A block no longer contains its own entry test.** `total_weeks` was
`duration_weeks + 1` and week 1 was an entry test, which made a duration mean two
things in exactly the way linear's did before 0013. 0013's table says a block
"requires a preceding one as input", and its own worked example puts the
standalone test on 18 September with the block opening on 21. So the entry test
is the standalone test before it, or the previous block's exit test, and a
block's weeks are its phase weeks.

**2. A test carries the repetition count it is performed at.** `block.rs` enters
a block on a triple deliberately — a cold maximal single measures technique as
much as strength — and exits on a single. Once the entry test is a programme of
its own, that count has to live on the test. `Block::entry_reps` is therefore
gone: by the time a block reads the number it is an `Anchor` and already a
one-rep maximum, and the check that the maximum table can convert it followed the
number to `Test`.

**3. A block requires `AnchorProvenance::Tested`; linear accepts any.** This is
what makes 0013's table a rule rather than advice. Rows three and six say a lift
change into a block needs a standalone test, and if an asserted anchor satisfied
the entry requirement the operator could skip that test by stating a number.
Linear accepts any provenance because a linear programme may declare its opening
outright.

**4. A test's fills are resolved when the document is read.** A test document
names the lift being tested and any fill moving with it; every slot it omits
comes from the programme before it. That resolution happens at authoring, so what
is stored is a complete `SlotFills` exactly as a linear programme's is.

**5. The test week's other session is the predecessor's.** The heavy session is
the test; the light one runs the predecessor's primary at the load its
progression stands at.

## Why

**Because a test is not a degenerate progression.** The first cut proposed here
was one `Programme` struct with a `Plan` enum field, which would have made a test
a linear programme with the ladder removed. The operator's cut names what the
categories actually are, and the code got shorter for it: `Periodisation` has one
`anchor`, one `gating_role`, one `entry`, and `Programme` has neither.

**Because the operator will not author a whole programme for two sessions.**
That was the stated objection and it shaped 4 and 5 between them. A test document
is nineteen lines of settled values; everything else — seventeen slot fills, the
warm-up ramp, six load scales — is inherited.

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
