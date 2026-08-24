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

### An adapter question, not a vocabulary one

Three of the operator's choices name implements the vocabulary has never had —
an EZ bar and a Smith machine — and Hevy has no template for any of them.

**That does not constrain our catalogue.** § 8 puts the vocabulary on this side
and the mapping in the adapter, so `preacher-curl-ez` exists here if an EZ bar
preacher curl is a different exercise from a barbell one — which it is, on the
same grounds that already keep a *dumbbell* preacher curl apart from a barbell
one. What Hevy lacks is the adapter's problem.

The adapter has two ways to answer it:

1. **Create a custom template in Hevy.** There is precedent: the operator did
   exactly this on 2026-08-20 for four movements Hevy had no exercise for, and
   `hevy::mapping` documents them.
2. **Map to the nearest thing Hevy offers** — `preacher-curl-ez` writes to
   *Preacher Curl (Barbell)*, and we accept that the source's record is less
   precise than ours.

### Why the second one does not work for a prescribed exercise

It reads well in the writing direction and breaks in the reading one.

Delivery would send `preacher-curl-ez` to Hevy's `4F942934`. The session is
performed, lands, and normalises — and `4F942934` maps back to
`preacher-curl-barbell`, because that is what the template says and
`hevy::mapping` is explicit that reading it as the movement the operator *meant*
would be the table asserting something the source never said.

So the loop does not close. History for `preacher-curl-ez` stays empty, double
progression never sees a performance, and the slot reports underivable forever.

Remapping `4F942934` to `preacher-curl-ez` instead is worse: it relabels the 121
sets of genuine barbell preacher curl already in the record.

**So a custom template is the practical answer for anything we intend to
prescribe.** The nearest-thing compromise is fine for an exercise we only ever
*read* — which is how the four stand-ins in `hevy::mapping` got there — and not
for one we write.

### What is actually left to decide

Only this: **is the distinction real to the operator?** If an EZ bar preacher
curl and a barbell one are the same exercise as far as his training is
concerned, there is nothing to add and nothing to map. If they are different, he
creates three templates in Hevy and the vocabulary gains three exercises, an
`Implement::EzBar` and an `Implement::SmithMachine` — each needing a scale in
`[parameters.scales]`, because an EZ bar and a Smith machine load differently
from a barbell.

Nobody else can answer that.
