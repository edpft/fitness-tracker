# Candidate exercises per slot

What the operator would consider for each slot, stated on 2026-08-24. The
programme setup wizard offers these — **ordered by what the record shows has
been performed, but not limited to it**, so a slot can be filled with something
new.

This is preference, not a domain fact. That a leg extension is knee-dominant is
true of anyone; that these are the three the operator would pick is true of him.
The two are worth keeping apart if this tool ever has a second user.

## Knee dominant

Which list applies depends on whether the programme makes this pattern its
**primary** — the slot that gets the ladder, the warm-up ramp and the back-offs
— or its accessory.

**As primary**

| | in the vocabulary |
|---|---|
| Back Squat (Barbell) | `squat-barbell` |
| Front Squat (Barbell) | `front-squat` |
| Bulgarian Split Squat (Barbell) | **missing** |

**As secondary**

| | in the vocabulary |
|---|---|
| Leg Extension (Machine) | `leg-extension-machine` |
| Bulgarian Split Squat (Dumbbell) | **mismodelled** — see below |
| Sissy Squat (Smith Machine) | **missing** |

## Hip dominant

**As primary**

| | in the vocabulary |
|---|---|
| Deadlift (Barbell) | `deadlift-barbell` |
| Romanian Deadlift (Barbell) | `romanian-deadlift-barbell` |

**As secondary**

| | in the vocabulary |
|---|---|
| Back Extension (Machine) | `back-extension-machine` |
| Nordic Hamstring Curl | `nordic-hamstrings-curls` |
| Leg Curl (Machine) | `seated-leg-curl-machine` |
| Lying Leg Curl (Machine) | `lying-leg-curl-machine` |

## Upper pull

| | in the vocabulary |
|---|---|
| Neutral Grip Pull-up | `neutral-grip-pull-up` |
| Overhand Grip Pull-up | `pull-up` |
| Ring Row | `ring-rows` |
| Bent-over Row | `bent-over-row-barbell` |
| Pendlay Row | `pendlay-row-barbell` |

## Upper push

| | in the vocabulary |
|---|---|
| Chest Dip | `chest-dip` |
| Bench Press (Barbell) | **missing** |
| Standing Overhead Press (Barbell) | `overhead-press-barbell` |

## Triceps

| | in the vocabulary |
|---|---|
| Single Arm Overhead Tricep Extension (Dumbbell) | `single-arm-tricep-extension-dumbbell` |
| Overhead Tricep Extension (Cable) | `overhead-triceps-extension-cable` |
| Skullcrusher (EZ) | **missing** |

## Biceps

| | in the vocabulary |
|---|---|
| Preacher Curl (EZ) | **missing** |
| Bayesian Curl (Cable) | `behind-the-back-curl-cable` — same movement, named descriptively |
| Incline Curl (Dumbbell) | `seated-incline-curl-dumbbell` |

---

## What has to happen before the wizard can offer all of these

### Straightforward: Hevy has them, we do not

Adding an exercise means three edits — the vocabulary in
`domain::gym::exercise`, the forward mapping in `hevy::mapping`, and the
reverse in `hevy::writable`. Without the last two it cannot be read back or
delivered.

| | Hevy template |
|---|---|
| Bench Press (Barbell) | `79D0BB3A` |
| Bulgarian Split Squat (Barbell) | `0F24286A` |

### A modelling error to correct

`bulgarian-split-squat` is declared **`Bodyweight`**, and the Hevy template it
maps from — `B5D3A742` — is *Bulgarian Split Squat (Dumbbell)*. So the implement
is wrong, and the barbell variant has nowhere to go. The operator wants the
barbell and dumbbell variants as distinct fills, which is the same rule that
already keeps a barbell and a dumbbell preacher curl apart.

### Needing a decision: implements we do not have

Three of the operator's choices name implements the vocabulary has never had,
and **Hevy has no template for any of them either**:

| wanted | Hevy offers |
|---|---|
| Skullcrusher (EZ) | Skullcrusher (Barbell) `875F585F`, (Dumbbell) `68F8A292` |
| Preacher Curl (EZ) | Preacher Curl (Barbell) `4F942934`, (Dumbbell) `FAB6EB2F`, (Machine) `1E9A6B8E` |
| Sissy Squat (Smith Machine) | Sissy Squat (Weighted) `F5DEF1EB` |

Three ways out, and it is the same choice each time:

1. **Treat them as the nearest thing Hevy has.** No new implement, no custom
   template — but the record then says "barbell" where an EZ bar was used, and
   § 8 says our vocabulary is ours rather than a source's.
2. **Add the implement and create a custom Hevy template.** Honest, and it makes
   the exercise readable and deliverable. Costs an `Implement` variant, a scale
   in `[parameters.scales]` for each — an EZ bar and a Smith machine load
   differently from a barbell — and a template created by hand in Hevy.
3. **Drop them from the candidate list**, if the nearest thing is what actually
   gets used.

An EZ bar is a different implement from a straight barbell on the same grounds
that a dumbbell preacher curl is a different exercise from a barbell one, so
option 1 is the one that costs something real. It is still the cheapest, and the
operator is the only one who knows whether the distinction matters to him.
