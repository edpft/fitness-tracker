# 0008 — The linear ladder climbs at a rate

**Date**: 2026-08-19
**Status**: Accepted
**Supersedes**: D8 in `specs/003-prescribed-workout-generation/research.md`, and
the "endpoint authored, step derived" rule in
`docs/primary-lift-progression.md`.
**Scope**: the `linear` template only. `block` derives its whole percentage
ladder from Prilepin's bands and a 105% exit (research D11, D12) and is
untouched.
**Raised by**: T080, the last open task of `specs/003-prescribed-workout-generation`.

## What was decided

**A linear block has no authored endpoint.** It is authored as an opening — a
percentage of the anchor — and a rate, and it adds that rate every climbing week
until the calendar stops it. What regulates the climb is the drop-and-re-climb
protocol, not a stated top.

```text
climbing weeks = duration - 1          the last week is the test
opening        = quantise(anchor × start)
heavy(w)       = quantise(opening + climb × (w - 1))
```

`GenerationParameters::ladder_end: Percentage` becomes
`ladder_climb_per_week: Kg`, and `generation_parameters.ladder_end_bp` becomes
`ladder_climb_grams` (migration `0009`).

## Why

**Because the operator says that is what the programme is.** Asked for the two
percentages T080 had been blocked on since the feature began, they answered:
"The linear progression doesn't target an exit percentage, it picks a starting
point and then attempts to add 2.5kg per week. The regulation comes from the drop
and re-climb protocol."

That is a statement about the programme, from the person whose programme it is,
and it is the kind of input § 9 puts on the data side of the line. The endpoint
was never that. It was reasoned to, in this repo, from published templates.

**The argument that was in the way, and why it does not hold.**
`docs/primary-lift-progression.md` rejected a weekly step twice over:

> An earlier version of this document had it advancing +2.5kg per week, which
> describes the same load sequence from the other end and cost the model its
> endpoint: a value that climbs indefinitely has no block to be the plan for.

> **The endpoint is authored and the weekly step is derived**, not the other way
> round. […] A weekly step is a number with nothing behind it, and multiplying
> it by a duration produces an endpoint nobody chose.

Both assume the *plan* is what has to regulate the climb — that if the arithmetic
does not stop, nothing does. In this model something else already stops it. A
miss holds the ladder, a second miss at the same load suspends it and drops back,
and the re-climb rejoins the plan where it left off. That mechanism was built
before this decision and is what "the regulation comes from the drop and re-climb
protocol" names. The endpoint was doing a job that was already done.

**The rate is not a number with nothing behind it.** 2.5kg is the smallest plate
in the operator's gym, and the reset protocol beside it is already stated in the
same unit — `reset1_reclimb_grams` at +5kg a week and `reset2_reclimb_grams` at
+2.5kg. The document even says the second reset "is the genuine slowdown: +2.5kg
weekly is baseline rate off a lower start", which names the baseline rate in
prose. The parameter existed in the model before it existed in the schema.

**It was already written down.** `specs/003-prescribed-workout-generation/HANDOVER.md`
has described the linear template as "Linear top-set/back-off, **+2.5kg**, reset
protocol" since 2026-08-18 — the rate and the regulator, in one line, in the
document written to tell the next person what the programme is. The code went on
saying "climbing to an authored endpoint" for another day because nobody read the
two side by side. A summary and the code disagreeing is a finding, not a typo.

**It also explains a symptom nobody had read as one.** The endpoint sat as
`TODO` in the authored document for the whole feature, refusing to author, and
three separate rounds of work went into deciding what number belonged there
without any of them landing. Nothing downstream ever needed the value for
anything except dividing it back into a step. A parameter that cannot be settled
and whose only use is to be divided away is a parameter the model does not have.

## What this costs, honestly

**Duration means less here than it did.** Under the span model the same endpoint
over 8 or 12 weeks was two different plans; now a longer block is the same plan
run further. That was the strongest argument for the endpoint and it is a real
loss.

It is the right loss for *this* template. `linear` is for the short and
interrupted window — the pre-Christmas mini-cut, a block broken by a holiday —
where "keep adding a plate for as long as we get" is the actual intent and a
duration-shaped plan is a fiction. Where duration genuinely shapes the plan is
`block`, and there it does: it sets the rung count and the phase split, and a
different duration really is a different programme. The two templates now differ
in a way that matches why each exists.

**Rows stored under the old model take the second reset's rate.** There is no
arithmetic from a span to a rate — the span was divided by a duration living in
another table — and reconstructing one would be fitting a parameter, which is the
error this decision is about. The conversion reads the documented identity
between the second reset's rate and the baseline instead. See migration `0009`.

## What is still open

**Where the ladder opens.** `[parameters.ladder] start` is still `TODO` in
`crates/infrastructure/tests/fixtures/programme.toml`, and authoring still
refuses a document carrying it. That is the remaining half of D8 and it is a
real operator input rather than something to be derived — the one number the
linear template asks for beyond the duration and the anchor.

## Consequences

- `Ladder::percentage` is gone. `implied_percentage` replaces it and is a
  reading of the plan for reporting, not an input to any derivation: the load is
  primary now, and the percentage is divided back out of it.
- `InvalidLadder::DoesNotRise` survives with a smaller job. A rate cannot
  descend — `Kg` is unsigned — so what is left to refuse is zero.
- Every step is exactly one rate. Under the span model the gap between two weeks
  depended on the anchor, the span and the duration together, and quantisation
  collapsed some pairs onto one bar; `every_step_is_the_authored_rate` in
  `crates/domain/tests/ladder.rs` now asserts what replaced that.
- The worked table in `docs/primary-lift-progression.md` is unchanged. It ran
  82.5, 85, 87.5, 90, 92.5, 95 from a 92.5% opening at a 90kg anchor, which the
  old model reached by a 2.25kg step that quantised back onto the plate and
  which this one reaches by adding the plate. The disclaimer that those numbers
  were a coincidence of the example can go.
