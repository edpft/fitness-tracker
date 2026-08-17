# Contract: the template's composition, and the authoring document

Two things: what `v1` will and will not build, and the TOML shape
`fitness programme author` reads.

The reasoning behind the template belongs to
[`docs/prescribed-workout-domain-model.md`](../../../docs/prescribed-workout-domain-model.md).
What is here is the composition as a contract — the slots that exist, their order,
and which groupings are structural rather than authored.

---

## The template: eleven slots, five blocks

Order is derived from the block, never authored. Nothing carries an index.

| # | Block | Slot | Grouping | Progression |
| --- | --- | --- | --- | --- |
| 1 | plyometric | `plyometric` | single | static |
| 2 | power | `power` | single | static |
| 3 | strength | `knee_dominant` | single | primary *or* double progression |
| 4 | strength | `upper_push` | supersetted with `upper_pull` | double progression |
| 5 | strength | `upper_pull` | supersetted with `upper_push` | double progression |
| 6 | strength | `hip_dominant` | single | primary *or* double progression |
| 7 | hypertrophy | `arms` | superset of two members | double progression |
| 8 | hypertrophy | `forearms` | superset of two members | double progression |
| 9 | hypertrophy | `core` | single | double progression |
| 10 | mobility | `mobility_hold` | single | static |
| 11 | mobility | `mobility_stretch` | superset of members | static |

**Issued order within the strength block is primary first**, then the upper pair
together, then the remaining lower slot as the accessory. So a knee-dominant
primary issues as front squat → (push + pull) → hip-dominant, which is what the
corpus shows in all fifteen sessions since 15 June.

### What is structural and unconstructible otherwise

- **Exactly one strength slot is primary.** `PrimaryPattern` is an enum, so two
  primaries and zero primaries are both unrepresentable.
- **The strength block requires all four patterns.** Named fields, so a block
  missing a hip-dominant slot does not compile.
- **The upper pair is supersetted; the lower pair is not.** Not a preference and
  not authored. The antagonist-pairing requirement needs no separate expression
  because the required pattern set already delivers a push against a pull.
- **The hypertrophy block is two supersets and one single slot.** `core` is typed
  as one exercise, so it cannot be supersetted. Fifteen consecutive sessions have
  it unpaired and last in the block.
- **Everything asymmetric derives from primacy.** The non-primary lower slot is
  accessory-style precisely because it is not primary; the upper pair are
  symmetric only because neither is.

### What the record varies and the template does not

Two variations appear in the corpus and neither becomes a template feature:

- **2026-08-14 grouped all five mobility exercises as one superset**, where the
  three sessions before it grouped three and left two single. Mobility grouping is
  a recording artefact rather than a prescription; the template issues hold, then
  the stretch superset, then the final stretch.
- **2026-08-14 substituted a single-arm dumbbell triceps extension** for the
  overhead cable extension used on 3, 7 and 10 August. A one-off substitution at
  the gym, not an alternation. The authored fill is the cable extension.

Recording both here because the generative test has to be able to say "the model
does not reproduce this, and that is correct".

### Composition of the plyometric and power blocks

Still no stated invariant — open question 3 in the domain model. Both are single
slots here, and the plyometric slot admits both a reps filler (pogos) and a
duration filler (jump rope), so a slot is not typed by its measure. That is a
consequence of the vocabulary partition, not a constraint this contract adds.

---

## The authoring document

TOML, read once, converted to `domain` types immediately (research D6). No `toml`
type reaches `domain`, and the `architecture` check verifies the ring.

The document below is the programme in force as far as the record shows it. **The
two `TODO`s are the ladder's span** — the one genuinely open value, research D8 —
and `fitness programme author` rejects the document while either remains, rather
than defaulting.

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

# One fill per slot. A slot taking one exercise takes a string; a slot that
# alternates by session role takes a table keyed by role.
[fills]
plyometric       = "pogo"
power            = "box-jump"
knee_dominant    = "front-squat"
upper_push       = "chest-dip"
upper_pull       = "pull-up"
core             = "cable-twist-up-to-down"
mobility_hold    = "handstand-hold"

# Alternates. The reason the history projection is unbounded (research D4): on
# any given session, the exercise being prescribed was last performed two
# sessions ago.
[fills.hip_dominant]
light = "back-extension-machine"
heavy = "nordic-hamstrings-curls"

[fills.arms]
members = ["preacher-curl-barbell", "overhead-triceps-extension-cable"]

[fills.forearms.light]
members = ["seated-wrist-extension-barbell", "seated-palms-up-wrist-curl"]

[fills.forearms.heavy]
members = ["reverse-wrist-curl-dumbbell", "seated-palms-up-wrist-curl"]

[fills.mobility_stretch]
members = ["dead-hang", "couch-stretch", "ninety-ninety", "stretching"]

# Generation parameters (§ 14). Only the current value is required, because the
# issued prescription records what these produced (SC-009).

[parameters]
back_off_of_top_set = "85%"
plate_increment     = "2.5kg"

# The ladder: where the heavy top set starts and finishes, as percentages of the
# anchor. The weekly step is DERIVED from these and duration_weeks, never
# authored — an endpoint is a claim about achievable gain, a step is a number
# with nothing behind it (research D2).
#
# This is the one genuinely open value (research D8). Bounds that exist:
#   - 5/3/1 embeds ~1.25kg/week for a lower-body lift
#   - a classic linear block finishes near 102.5-105% of entry
#   - a demonstrated ~99kg in Apr 2025 means ~9kg of regain before new ground
#   - a reset costs 4 of 7 climbing weeks, so leave room for one
[parameters.ladder]
start = "TODO"      # a classic block opens around 92.5%
end   = "TODO"      # 105% of 90kg is 94.5 -> 95kg

# The light session's top set, as a percentage of that week's heavy top set.
# 88.5% reproduces 72.5/75/77.5 against 82.5/85/87.5 across three validated
# weeks. A flat -10kg offset fits equally well and loses at a different anchor.
light_of_heavy = "88.5%"

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
