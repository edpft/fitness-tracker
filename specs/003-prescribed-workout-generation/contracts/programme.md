# Contract: the template's composition, and the authoring document

Two things: what `v1` will and will not build, and the TOML shape
`fitness programme author` reads.

The reasoning behind the template belongs to
[`docs/prescribed-workout-domain-model.md`](../../../docs/prescribed-workout-domain-model.md).
What is here is the composition as a contract — the slots that exist, their order,
and which groupings are structural rather than authored.

---

## The template: seventeen slots, five blocks, two variants

One template, in two variants. They differ in exactly one thing — whether the
primary lift is knee-dominant or hip-dominant — and the other lower slot is then
the accessory. The programme states which variant is in force for the block; it
does not vary session to session.

Order is derived from the block, never authored. Nothing carries an index.

| # | Block | Slot | Grouping | Progression |
| --- | --- | --- | --- | --- |
| 1 | plyometric | `plyometric` | single | static |
| 2 | power | `power` | single | static |
| 3 | strength | `knee_dominant` | single | primary *or* double progression |
| 4 | strength | `upper_push` | supersetted with `upper_pull` | double progression |
| 5 | strength | `upper_pull` | supersetted with `upper_push` | double progression |
| 6 | strength | `hip_dominant` | single | primary *or* double progression |
| 7 | hypertrophy | `biceps` | supersetted with `triceps` | double progression |
| 8 | hypertrophy | `triceps` | supersetted with `biceps` | double progression |
| 9 | hypertrophy | `wrist_flexion` | supersetted with `wrist_extension` | double progression |
| 10 | hypertrophy | `wrist_extension` | supersetted with `wrist_flexion` | double progression |
| 11 | hypertrophy | `core` | single | double progression |
| 12 | mobility | `handstand_hold` | single | static |
| 13 | mobility | `dead_hang` | single | static |
| 14 | mobility | `hip_flexor_stretch` | the stretch circuit | static |
| 15 | mobility | `hip_external_rotator_stretch` | the stretch circuit | static |
| 16 | mobility | `hamstring_stretch` | the stretch circuit | static |
| 17 | mobility | `groin_stretch` | the stretch circuit | static |

**Issued order within the strength block is primary first**, then the upper pair
together, then the remaining lower slot as the accessory. So a knee-dominant
primary issues as front squat → (push + pull) → hip-dominant, which is what the
corpus shows in all fifteen sessions since 15 June.

### What is structural and unconstructible otherwise

- **The primary is one lower lift.** `PrimaryPattern` has two variants, so two
  primaries, zero primaries, and an upper lift as primary are all
  unrepresentable, and the accessory lower slot is total rather than optional.
- **Every slot is named as a field.** A programme missing a fill does not
  compile.
- **A superset is exactly two named slots.** `Position::Superset` holds a pair,
  so no slot holds a bag of members and no authored list can lengthen one. Which
  slots pair is the template's, not the programme's: push against pull, biceps
  against triceps, wrist flexion against extension.
- **The four stretches are one circuit.** `Position::Circuit` is its own case
  rather than a widened superset, because it is a fixed group of four and not an
  antagonist pairing. The two holds stay separate items.
- **A slot holds one exercise.** The only freedom the programme has is which
  exercise it picks to work that slot's target — preacher curl or Bayesian curl
  in `biceps`, not two biceps exercises and no triceps.
- **Everything asymmetric derives from primacy.** The non-primary lower slot is
  accessory-style precisely because it is not primary; the upper pair are
  symmetric only because neither is.

### The template is intent, not a summary of the record

The corpus predates this template. It records a programme run by hand whose
composition changed while it ran, so where the two differ the template is not
wrong — it is the thing the record is about to start conforming to. Two
differences are known and deliberate:

- **Mobility grouping.** The record groups the stretches inconsistently — three
  as a superset and one trailing single, or all five as one. The template says
  two holds and then one circuit of four, which is what the operator intends
  from here.
- **The forearm slots.** The record alternates the wrist extension exercise by
  session role. That was habit rather than plan; the settled fills are the
  dumbbell wrist flexion and the dumbbell wrist extension, one each, both
  sessions.

Neither is a reason to widen the template, and neither is asserted against the
corpus. The property that matters runs forward: a session performed against a
generated prescription projects back into one, which
`domain/tests/projection.rs` asserts over generated sessions.

### Four exercises are mapped and not yet performed

Hevy had no exercise for four of the movements the template names, so the
operator created them on 2026-08-20 and `hevy/mapping.rs` now carries all four
template ids. Each load reading is the Hevy template's own declared `type`, not a
judgement: `Neutral Grip Pull Up` is `bodyweight_assisted` and so negates, like
every other assisted variant.

**The stand-ins keep reading as what they say.** Three of these were being logged
under the nearest thing Hevy offered — `Pull Up (Assisted)`, `Cable Twist (Up to
down)`, `Stretching` — and those templates still translate to those exercises.
Reading a template as the movement he *meant* would be the adapter asserting
something the source never said. Correcting the workouts that used a stand-in is
the edit overlay's job.
`a_stand_in_template_still_reads_as_the_exercise_it_names` holds that line, and
`the_newly_created_templates_resolve` covers the four new ids, which nothing else
touches until the first session uses one.

**Until they are performed, their slots are reported rather than guessed.**
Double progression has no last performance to progress from, so `upper_pull` and
`core` come back as `NeverPerformed` (FR-011), and `upper_push` comes back as
`GroupWithheld` — it derives perfectly well and is absent only because half a
superset is not the template's item. The two stretches are holds, which need no
history, so they issue from the first session.

### Composition of the plyometric and power blocks

Still no stated invariant — open question 3 in the domain model. Both are single
slots here, and the plyometric slot admits both a reps filler (pogos) and a
duration filler (jump rope), so a slot is not typed by its measure. That is a
consequence of the vocabulary partition, not a constraint this contract adds.

---

## The authoring document

TOML, read once, converted to `domain` types immediately (research D6). No `toml`
type reaches `domain`, and the `architecture` check verifies the ring.

The document below is the programme in force as far as the record shows it. **It
carries no `TODO`** — D8's two unauthored values were dissolved rather than filled
in on 2026-08-19 (research D13 and D14). `fitness programme author` still rejects
a document carrying one, rather than defaulting; the `TODO`s remaining below are
in the warm-up ramp, which this contract has never settled.

Everything else is now evidenced: the anchor is the 3 July test, the light-of-heavy
percentage reproduces three validated weeks, and the rep counts have been constant
per role since July.

```toml
# The programme. Authored intent (§ 12), not a derivation.

[programme]
template          = "v1"
primary           = "knee_dominant"
primary_exercise  = "front-squat"
gating_role       = "heavy"
start             = "2026-07-06"
# Weeks, not cycles. The last one is the test, so this is 7 climbing weeks.
duration_weeks    = 8

# Which weekday is which session role (research D7). A date on no listed
# weekday is an error naming these days, never an implicit nearest match.
[programme.weekdays]
monday = "light"
friday = "heavy"

# The starting 1RM. Fixed for the whole block; only the exit test replaces it,
# and that replacement anchors the *next* block (research D2).
[programme.anchor]
load       = "90kg"
provenance = "tested"
from       = "2026-07-03"

# One exercise per slot. A slot takes a string; a slot that alternates by session
# role takes a table keyed by role. No slot takes a list — which slots superset
# together is the template's, not the programme's.
[fills]
plyometric                   = "pogo"
power                        = "box-jump"
knee_dominant                = "front-squat"
upper_push                   = "chest-dip"
upper_pull                   = "neutral-grip-pull-up"
biceps                       = "preacher-curl-barbell"
triceps                      = "overhead-triceps-extension-cable"
wrist_flexion                = "wrist-flexion-dumbbell"
wrist_extension              = "wrist-extension-dumbbell"
core                         = "bent-over-cable-chop"
handstand_hold               = "handstand-hold"
dead_hang                    = "dead-hang"
hip_flexor_stretch           = "couch-stretch"
hip_external_rotator_stretch = "ninety-ninety"
hamstring_stretch            = "standing-straddle-fold"
groin_stretch                = "squatting-groin-stretch"

# Alternates. The reason the history projection is unbounded (research D4): on
# any given session, the exercise being prescribed was last performed two
# sessions ago.
[fills.hip_dominant]
light = "back-extension-machine"
heavy = "nordic-hamstrings-curls"

# Generation parameters (§ 14). Only the current value is required, because the
# issued prescription records what these produced (SC-009).

[parameters]
back_off_of_top_set = "85%"
plate_increment     = "2.5kg"

# The ladder: what the heavy top set adds each climbing week. The step is
# AUTHORED and there is no endpoint — the climb runs until the calendar stops it,
# and the reset protocol regulates it (research D13, decision 0008). This
# reversed research D2, which had the endpoint authored and the step derived.
#
# Where it opens is not here: it comes from the entry test in [programme.anchor]
# (research D14, decision 0009).
[parameters.ladder]
climb_per_week = "2.5kg"   # the smallest plate, and the second reset's rate

# The light session's top set, as a percentage of that week's heavy top set.
# Stated by the operator on 2026-08-18. It was 88.5%, fitted to three light and
# heavy pairs that are a flat -10kg apart — a ratio drawn through an offset,
# which drifts across the three where the offset does not. 85% is one plate
# lighter and is a decision.
light_of_heavy = "85%"

# Percentages of the top set, with their rep counts. Four steps, matching the
# ramp the record shows; the percentages themselves are approximate there and
# are authored here rather than fitted.
[[parameters.warmup]]
of_top_set = "TODO"        # ~40% observed
reps       = 4
[[parameters.warmup]]
of_top_set = "TODO"        # ~60% observed
reps       = 3
[[parameters.warmup]]
of_top_set = "TODO"        # ~80% observed
reps       = 2
[[parameters.warmup]]
of_top_set = "TODO"        # ~90% observed
reps       = 1

# Both roles must be present; `PerRole` is a struct, so a missing role is a
# compile error rather than a runtime one.
#
# Constant within a block. Descending reps across the block — fives, threes,
# singles — is the textbook linear variant and is deferred, because the record
# has held these fixed per role since the July test while the load climbed.
[parameters.roles.light]
top_set_reps = 3

[parameters.roles.heavy]
top_set_reps = 1

# From primary-lift-progression.md. Drop and re-climb are chosen as a pair so
# both land on the plate grid and both cost four weeks.
[parameters.reset.first]
drop             = "-10%"
reclimb_per_week = "5kg"

[parameters.reset.second]
drop             = "-5%"
reclimb_per_week = "2.5kg"
```

### Why the document carries units as strings

`"2.5kg"`, `"85%"`, `"-10%"`. Parsed into `Kg` and `Percentage`, which are
fixed-point integers — no float on the path, inherited from 002's reasoning about
loads. A bare `2.5` in TOML is a float, and a percentage that round-trips
differently across builds makes a stored prescription unreproducible. The suffix
also makes the document readable, which is its only reason to exist over flags.

### Validation, in order

1. **TOML parses.** Adapter-level; a syntax error names line and column.
2. **Every value converts.** `"2.5kg"` into `Kg`, `"front-squat"` into the reps
   vocabulary, `"monday"` into a weekday. An unknown exercise key names itself and
   the vocabulary it was looked up in.
3. **No `TODO` remains.** Rejected with the field path, because a placeholder that
   authors successfully is worse than one that fails.
4. **The types take over.** `SlotFills` totality, `PerRole` completeness,
   `Range` bounds — all compile-time or construction-time.
5. **The three consistency checks** in [ports.md](./ports.md): the gating role
   appears in the weekday mapping, the primary exercise is in the reps vocabulary,
   and the primary exercise is the fill of the slot named by `primary`.

Steps 1 to 3 are the adapter's. Steps 4 and 5 are `domain` and `application`, which
is the split that keeps the document format out of the core.
