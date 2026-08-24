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
| Bulgarian Split Squat (Barbell) | **to add** — Hevy `0F24286A` |

**As secondary**

| | in the vocabulary |
|---|---|
| Leg Extension (Machine) | `leg-extension-machine` |
| Bulgarian Split Squat (Dumbbell) | **mismodelled** — see below |
| Sissy Squat (Smith Machine) | `sissy-squat` — an EZ bar and a Smith machine are not distinct implements here; see below |

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
| Bench Press (Barbell) | **to add** — Hevy `79D0BB3A` |
| Standing Overhead Press (Barbell) | `overhead-press-barbell` |

## Triceps

| | in the vocabulary |
|---|---|
| Single Arm Overhead Tricep Extension (Dumbbell) | `single-arm-tricep-extension-dumbbell` |
| Overhead Tricep Extension (Cable) | `overhead-triceps-extension-cable` |
| Skullcrusher (EZ) | **to add** as a barbell skullcrusher — Hevy `875F585F` |

## Biceps

| | in the vocabulary |
|---|---|
| Preacher Curl (EZ) | `preacher-curl-barbell` — same total load, same exercise |
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

### The implement question, settled

Three of the operator's choices name implements the vocabulary has never had —
an EZ bar and a Smith machine. Hevy has no template for them either, which was
briefly treated here as a constraint. **It is not**: § 8 puts the vocabulary on
this side and the mapping in the adapter, so an exercise exists here if it is a
distinct exercise, whatever Hevy holds.

The operator settled it on 2026-08-24, and the reason is sharper than the
question:

> There isn't really a distinction between an EZ bar and a barbell because
> we're recording total load.

**That is the test.** An implement makes a different exercise when it changes
what the recorded number *means*, not when it changes how the bar feels. A
dumbbell preacher curl is distinct because the load is per hand; an EZ bar
preacher curl is not, because forty kilograms is forty kilograms.

So there is no `Implement::EzBar`, no `Implement::SmithMachine`, and no custom
Hevy template to create:

| wanted | resolves to |
|---|---|
| Preacher Curl (EZ) | `preacher-curl-barbell` — already exists |
| Skullcrusher (EZ) | a barbell skullcrusher — **to add**, Hevy `875F585F` |
| Sissy Squat (Smith Machine) | `sissy-squat` — already exists |

### The whole of what is left

| | Hevy template |
|---|---|
| Bench Press (Barbell) | `79D0BB3A` |
| Skullcrusher (Barbell) | `875F585F` |
| Bulgarian Split Squat (Barbell) | `0F24286A` |
| Bulgarian Split Squat (Dumbbell) | `B5D3A742` — currently mismodelled |

Adding an exercise is three edits: the vocabulary in `domain::gym::exercise`,
the forward mapping in `hevy::mapping`, and the reverse in `hevy::writable`.
Without the last two it can be neither read back nor delivered.

### One thing the reading direction would have made expensive

Worth keeping, because it is the argument that would have applied had the EZ
distinction been real.

Mapping one of our exercises to *the nearest thing* Hevy offers reads well going
out and breaks coming back. Delivery would send `preacher-curl-ez` to Hevy's
`4F942934`; the session is performed, lands, and normalises — and `4F942934`
maps back to `preacher-curl-barbell`, because that is what the template says and
`hevy::mapping` is explicit that reading it as the movement the operator *meant*
would be the table asserting something the source never said.

So the loop would not close: history for the prescribed exercise stays empty,
double progression never sees a performance, and the slot reports underivable
forever. Remapping the template the other way is worse — it relabels the 121
sets of genuine barbell preacher curl already in the record.

**The nearest-thing compromise is for exercises we only ever read**, which is
how the four stand-ins in `hevy::mapping` got there, **and never for one we
prescribe.**
