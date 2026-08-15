# Contract: the exercise mapping

The mapping itself is `crates/infrastructure/src/hevy/mapping.rs` — 134 template
ids onto 128 exercises, code rather than data (§ 9). This document holds the
rules it was authored against. The table is not reproduced here: a rule stated
twice is a rule that drifts, and the code is the one that runs.

## What an entry resolves

Per `exercise_template_id`, and nothing else:

1. **Which of our exercises** it is — many-to-one.
2. **Which measure** that exercise is in, which follows from the vocabulary it
   belongs to and so is not a separate field.
3. **How to read the weight column**: `Absolute`, `Relative`, or
   `RelativeNegated`.

Titles inform the mapping and never key it. `Overhead Squat` has two template
ids — a builtin and a custom — and template `DDB29047` has appeared under two
titles, having been renamed mid-history. Both are covered because both are keyed
on the id.

## The rules

**A variant that differs only in how the movement is loaded is not a different
movement.** Assisted and unassisted are one exercise; weighted and unweighted
are one exercise. This is the mapping's reason to exist (§ 8), and it is what
makes `Pull Up` (97 sets), `Pull Up (Assisted)` (159) and `Pull Up (Band)` (3)
one series rather than three.

**An assisted variant negates.** Hevy has no assistance concept — assisted
movements are separately-named exercises carrying a positive weight — so
`RelativeNegated` turns 20 into −20 and puts assistance and added weight on one
axis.

**Load is `Relative` where assistance is conventionally available**, so the axis
runs through zero in both directions and the sign carries meaning. `Absolute`
otherwise, where the number is external load and none of it is a real answer.

This is a convention about how a movement is trained, not a physical fact about
it, which is why it is decided here per exercise and never inferred from a
value. A squat has a bodyweight version and is still `Absolute`: adding weight is
the whole progression and taking weight away is not a thing anyone does. A
pull-up is `Relative` because both directions are routine. Three families are
`Relative` today — pull-up, chin-up, chest dip — and the list grows by editing
this table, never by a record arriving.

**Band resistance is load; band assistance is negative.** A banded lateral raise
reads its number as external load like any other. A banded pull-up takes weight
off, so it negates. Neither refuses: the number is the operator's estimate of the
band, and discarding it forecloses an overlay supplying a resistance range later.

What stays a declared limitation is comparability — the account's assisted loads
run `0, 7, 14, 21, 28, 35, 42`, stacked bands rather than a machine stack, and
deterministic translation cannot tell the two apart. Same axis, not the same
series.

**Where our category and the source's differ, ours wins.** Two entries.
`Sled Push`, which Hevy calls distance-and-duration and which records thirty
seconds and a zero distance on all nine sets, is a duration exercise here.
`Running`, which records a distance and a duration, is a distance exercise: the
duration was an interval target rather than a measurement.

## What the mapping does not decide

**What was actually performed**, where the operator has used a template to stand
for something else. Six such cases are recorded in the model of record under
"Known aliases, for the edit overlay". A template does not determine the
movement, so a deterministic mapping does not pretend it does — those wait for
the overlay, where an operator assertion is the right kind of input.

**Whether an implement has irreducible mass.** A barbell always weighs
something, so a barbell exercise recording no external load is recording lossily.
The mapping could say so and does not: nothing consumes it, and it was the
accidental basis of the rule this one replaces
([decision 0004](../../../docs/decisions/0004-the-load-axis-is-bidirectional-or-it-is-not.md)).

**`ExerciseTemplate.type`**, Hevy's declared exercise type. It is the only
published carrier of the sign convention and is invisible in a workout payload,
so it informed this table when it was authored and is not read when translation
runs. Reading it then would put a network request inside a derivation that must
not make one, and would make the result depend on what the vendor's catalogue
says today rather than on what the record holds.

## How the mapping is held to account

**Every template resolves.** All 134 ids in the corpus reach an entry, and an id
that does not stops the run naming itself. The vocabulary is code, so a gap in it
is a defect to fix rather than data to record around.

**The collapses hold.** The pull-up family's three templates and the chest dip's
two each reach one exercise; a weighted crunch reaches the same exercise as a
plain one, and a weighted hyperextension the same as an unweighted one.

**The axis readings are right.** A squat and a deadlift are `Absolute`; a pull-up
and a dip are `Relative`. Asserted directly, because a wrong reading no longer
shows up as a refusal — under the previous rule it did, which was the accident
that made it look self-checking.
