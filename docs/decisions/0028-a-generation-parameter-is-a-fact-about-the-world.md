# 0028 — A generation parameter is a fact about the world, not about a programme

**Date**: 2026-09-02

**Amends the constitution**: § 14 narrowed, § 14.1 added. Version 1.0.1 → 2.0.0.

**Follows from**: `0026-do-not-mix-bounded-contexts.md`. This is that rule
applied to a case where it costs something.

## Context

`fitness parameters` and `GenerationParameters` were built to hold values that
could change: plate increments, back-off percentages of the top set, warm-up
repetitions and percentages. The operator, 2026-09-02:

> "fitness parameters may also have been a mistake. the idea was to store values
> that could change, like weight plate increments, back off set of top set
> percentages, warm up reps and percentages. again, in actually build this, what
> we found was that these aren't really parameters, apart from plate increments,
> they are part of the programme itself."

**This was a conflict with the constitution rather than an observation about
code.** § 14 named *"warmup set percentages"* as its first example of a
generation parameter, so the module was conforming and the rule was wrong.
Raised explicitly under Governance and settled by amendment.

**Why the rule was wrong is 0026.** A warm-up ramp is *SBS's* warm-up ramp. One
set of back-off percentages consulted by SBS, by *Peak Your Power Zones* and by
anything Friel-shaped is a shared model of training across programmes that do
not share one — the exact thing 0026 forbids, arrived at from the other
direction.

## Decision

**A generation parameter is a fact about the world the programme is run in,
never a fact about how the programme prescribes.** The operator's own
formulation, 2026-09-02:

> "available plates increments stays because it's a fact about the world that we
> have to programme to, like my training availability, the other 'variables' are
> part of their programmes."

**The test**: would the value still be true if no programme were being authored?

| | still true | verdict |
|---|---|---|
| available plate increments | yes — it is what is on the rack | parameter |
| training availability | yes — it is what his week is | parameter |
| the family calendar | yes | parameter |
| back-off percentage of the top set | no | the programme's |
| warm-up repetitions and percentages | no | the programme's |
| top-set repetitions | no | the programme's |
| reset protocol | no | the programme's |

Plate increments and training availability sit together and that is the point of
the pairing: both are constraints the programming has to accommodate, and
neither is an opinion about training. The schedule already holds the second, and
has since #27.

## Consequences

- **`GenerationParameters` splits.** `Scales` — load steps per implement — is
  the parameter and stays. `WarmupStep`, `BackOff`, `TopSetReps`,
  `ResetProtocol` and `AccessoryScheme` stop being parameters. Where the
  published programme states them they become the programme's; where it is
  silent they become fixed, which is the next section.
- **`fitness parameters show` narrows to what remains**, or goes. It reports
  *"every parameter in force"* and most of what it currently reports will not be
  a parameter.
- **The store's parameter tables split the same way**, and the programme's half
  becomes part of what a programme is.
- **0027's open question is answered, in the negative.** It asked whether
  asserting a maximum might be a `parameters` verb. It is not: a maximum is a
  measurement, which § 12 makes authored data and § 13 makes interpretive and
  effect-dated. It belongs with the record. Where it is entered is still open;
  that it is not a generation parameter is now settled.

## The original sin was flexibility, not location

A first draft of this decision said these values "move into the context of the
programme that prescribes them", and treated it as open what happens when the
publisher is silent — SBS's notes give no warm-up ramp, and the handover of
2026-09-02 records that the ramp toward a rep max is an agent's derivation
nobody reviewed. The operator closed it and rejected the framing with it,
2026-09-02:

> "a warm up ramp isn't technically part of a programme, if that programme
> doesn't define one, but, what I mean is, it's something you expect to be
> determined by programming (more broadly defined). the original sin here was
> trying to make the programming element flexible when it doesn't need to be. I
> have a set workout shape, we're not building an arbitrary workout building,
> similarly, I have a set warm up ramp."

**So the question was never where to keep the knob. It was that there is no
knob.** He has one warm-up ramp and one workout shape. A value with a single
value is a constant, and moving it from a settings table into a per-programme
field would have preserved the mistake while appearing to fix it — the same
error the aborted block made, inventing flexibility the material did not have.

This is the wider form of 0026. There, structure was imported from a foreign
context; here, structure was invented from nothing. Both produce a model with
more degrees of freedom than the thing it describes.

**Still to settle, and he named it**: *"there is some need to iron out exactly
what that should look like for non 1RM tests, but once we've done that it's
set."* The ramp toward an 8RM, a 5RM and a 3RM is undecided — see the rep-max
work-up question raised on 2026-09-02, which research found has no published
protocol to adopt. It is one decision, taken once, and then a constant.
