# 0004 — The load axis is bidirectional, or it is not

**Date**: 2026-08-14
**Status**: Accepted
**Supersedes**: [0003](0003-unrecorded-resistance-translates-as-relative-zero.md).
**Revises**: `docs/gym-workout-domain-model.md` — "Absolute vs Relative is
decided by whether zero is performable", and the seven zero loads it calls wrong
data.
**Raised by**: review of `specs/002-hevy-workout-normalisation`.

## What was decided

`Absolute` and `Relative` are chosen by asking whether assistance is
conventionally available for the movement, not by asking whether a zero could be
performed. `Absolute` admits zero, which means no external load.

## Why the previous rule was wrong

It read a convention as a physical fact.

"Zero is performable" sounds like a property of the movement, and for a barbell
squat the answer looks like no — there is always a bar. But a bodyweight squat is
obviously a thing. What is actually true about a squat is that *taking weight
away* is not part of how anyone trains it: you add weight, and the number is how
much you added. A pull-up is the opposite. The bodyweight version is the
movement, assistance is routine — a machine, a band, a partner — and so is
loading it with a belt or a dumbbell. The axis runs both ways and the sign
carries meaning.

So the question is about the axis, and the answer is a convention. That is why
it is decided per exercise in the mapping rather than inferred from any value,
and why an exercise that becomes conventionally assisted moves without any data
moving.

## What this costs, and it is the interesting part

**The seven zeros stop being wrong data.** The old rule made a zero on a barbell
exercise an error *by construction*, and the model document narrates those seven
— an empty bar on a good morning, a PVC pipe on an overhead squat — as the
clearest example of a model rejecting a bad record. Under the new rule they are
simply sets with no external load, and they translate.

That was the strongest argument for keeping the old rule, and it does not
survive contact with what the rule was doing. It diagnosed those seven correctly
by accident: what makes `0, 5, 10` on a good morning look wrong is knowing that
a barbell has mass, which is a fact about the *implement* rather than about the
direction of the load axis. Encoding it as the axis rule meant one distinction
carrying two jobs, and the second job was the one being tested.

A third thing the mapping declares — whether an exercise's implement has
irreducible mass — would recover the diagnosis honestly. It was offered and
declined: nothing consumes it yet, and inventing a field to keep a test alive is
the wrong order.

**The corpus now refuses one set.** 3,778 of 3,779 translate, and the one that
does not is the missed attempt — 95 kg for zero reps — which is the single case
the model document identified as a genuine gap. A model that rejects exactly the
thing it has no shape for is a better result than one that rejects twenty-four
things for three different reasons.

## Consequences

- `Load::Absolute` is infallible and `Kg` is unsigned. There is no
  `ZeroOnAbsoluteLoad` refusal, and the seven-zero test is gone.
- Only the pull-up, chin-up and chest-dip families are `Relative` in the current
  mapping. Everything else is `Absolute`, where a missing weight is no external
  load rather than a missing value.
- The air bike needs no special case: it carries no external load, and
  `Absolute(0)` says exactly that. **The sled still does** — it has plates on it
  and the number is not recorded — so `Absolute(0)` understates it. That is a
  declared limitation, and it is now one exercise rather than two.
- Band resistance translates: the recorded number is read as external load like
  any other. Band *assistance* still negates onto the relative axis. Refusing
  banded sets outright would have foreclosed an overlay supplying a resistance
  range later, which is the point of recording the number at all.
