# Contract: the exercise mapping

Phase 1. The mapping itself is `crates/infrastructure/src/hevy/mapping.rs` — 134
entries, code rather than data (§ 9). This document holds the rules it was
authored against and the entries where a rule had to be applied rather than
read off. The table is not reproduced here: a rule stated twice is a rule that
drifts, and the code is the one that runs.

## What an entry resolves

Per `exercise_template_id`, and nothing else:

1. **Which of our exercises** it is — many-to-one, so `Pull Up`, `Pull Up
   (Assisted)` and `Pull Up (Band)` all reach `PullUp`.
2. **Which measure** that exercise is in, which follows from the vocabulary the
   exercise belongs to and so is not a separate field.
3. **How to read the weight column**: `Absolute`, `Relative`, `RelativeNegated`
   (an assisted variant), or `BandResistance` (refused).

Titles inform the mapping and never key it. `Overhead Squat` has two template
ids — a builtin and a custom — and template `DDB29047` has appeared under two
titles, having been renamed mid-history. Both are covered because both are keyed
on the id.

## The rules

**Load is `Absolute` where no unloaded version of the movement exists.** The
implement has mass, so zero is impossible and a zero is a data error by
construction. `Relative` where an unloaded version does exist, so zero is a real
observation and the number is a delta against a bodyweight the set does not
record.

**An assisted variant negates.** Hevy has no assistance concept — assisted
movements are separately-named exercises carrying a positive weight — so
`RelativeNegated` turns 20 into −20 and puts assistance and added weight on one
axis. This is the mapping's reason to exist (§ 8).

**A band-resistance exercise refuses.** Band tension varies through the range of
motion, nothing records the mechanism, and the account's assisted loads run `0,
7, 14, 21, 28, 35, 42` — stacked bands rather than a machine stack, which
deterministic translation cannot tell apart. Four templates, 16 sets:
`Banded Scapula Protraction` (5), `Band Pullaparts` (5), `Front Raise (Band)`
(3), `Lateral Raise (Band)` (3).

`Pull Up (Band)` is **not** among them. It is band *assistance*, not band
resistance — 3 sets at a recorded 14 — and it maps to `PullUp` with
`RelativeNegated` like any other assisted pull-up. That band assistance and
machine assistance are not comparable is the limitation the model of record
declares; it is not a reason to refuse the set.

**Where our category and the source's differ, ours wins.** One entry:
`Sled Push`, which Hevy calls distance-and-duration and which records thirty
seconds and a zero distance on all nine sets. It is a `DurationExercise` here,
so the zero distance is never read and never refused.

## The entries a rule had to be applied to

Fourteen templates record at least one zero weight, and which of them are
`Absolute` is the whole judgement. Ten are `Relative`, so their zeros are plain
bodyweight and translate; four are `Absolute`, so their seven zeros refuse.

The pair that shows the rule is doing real work:

| | Load | Because |
| --- | --- | --- |
| `Romanian Deadlift (Barbell)` | Absolute | There is no barbell RDL without a barbell |
| `Single Leg Romanian Deadlift (Dumbbell)` | Relative | A single-leg RDL is a balance drill before it is a loaded hinge, and four sets in the corpus were done that way |

Nothing in the titles forces that split. What forces it is asking whether the
movement can be performed unloaded — and the answer differs, though both are
Romanian deadlifts and one names a dumbbell.

The other three `Absolute` templates carrying zeros are `Overhead Squat` (custom
id), `Snatch-Grip Behind The Neck Press` and `Good Morning (Barbell)`. The other
nine `Relative` ones are listed in [research.md](../research.md), D2.

## How the mapping is held to account

**Exactly seven zeros refuse.** Not a target the table is tuned to hit — it
falls out of asking "can this be done unloaded?" 134 times, and it is wrong in
both directions: an eighth refusal means a `Relative` movement was called
`Absolute`, and a sixth means the reverse. Pinned by
[quickstart.md](../quickstart.md) scenario 4.

**Every template resolves.** All 134 ids in the corpus reach an entry, and an id
that does not stops the run naming itself (FR-017). The vocabulary is code, so a
gap in it is a defect to fix rather than data to record around.

**The collapses hold.** `Pull Up` (97 sets) with `Pull Up (Assisted)` (159) and
`Pull Up (Band)` (3); `Chest Dip` (84) with `Chest Dip (Assisted)` (277). One
exercise each, one series each, assistance negative.

## What the mapping does not consult

`ExerciseTemplate.type`, Hevy's declared exercise type, is the only published
carrier of the sign convention and is invisible in a workout payload. It
informed this table when it was authored and is not read when translation runs
— which closes the question 001 left open. Reading it at translation time would
put a network request inside a derivation that must not make one, and would make
the result depend on what the vendor's catalogue says today rather than on what
the record holds.
