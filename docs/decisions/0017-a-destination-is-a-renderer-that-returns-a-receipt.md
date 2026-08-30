# 0017 — A destination is a renderer that returns a receipt

**Date**: 2026-08-23

## Context

Prescriptions ended at a printed session. Acting on one meant retyping it into
Hevy by hand, which is where the two arithmetic errors of August came from and
what the prescribed side was built to stop.

Writing back raises a question the domain model parked at
`docs/prescribed-workout-domain-model.md` under "Not modelled here": whether one
routine is rewritten in place each cycle or one is issued per prescribed
workout. It was parked as more than adapter tidiness, because rewrite-in-place
destroys the discriminating power of the `routine_id` that open question 1 wants
to match performances on.

Two facts about the source pull in the same direction and neither is a reason on
its own. Hevy publishes no `DELETE` for a routine or a folder, and it retires the
id of anything removed by hand — so an id that names a session stops naming it
if the routine is tidied away. Both are Hevy's constraints, and § II.3 is
explicit that no source's shape decides ours.

## Decision

**A destination is a renderer that returns a receipt**, and delivery is a driven
adapter behind a port like any other external system (§ 16).

Printing a session to a terminal and putting it in the app the operator trains
from are the same act: deriving what to do, and rendering it. Neither is part of
the domain's reasoning, and `WorkoutShape` was already the common currency. The
one asymmetry is that a terminal forgets and an app does not — it keeps what it
was given under an identity of its own — and that identity is the only residue
worth recording.

Three consequences follow.

**No layer applies.** A routine we created is not an observation of anything, so
it acquires no raw, normalised or canonical form. It is § 12 authored data: a
record of intent, extended by where that intent was sent. § II says in terms not
to invent a slot for what it does not reach, and this is that case.

**The reference is opaque above the adapter.** `DeliveryReference` is validated
against emptiness and nothing else, exactly as `SourceRecordId` is, and only the
Hevy adapter knows it is a UUID. The precedent is the resumption token: the
application carries it and never reads it.

**One delivery per issued prescription**, created and never updated. This falls
out of § 12 alone. An issued prescription is written once and never rewritten; a
reissue is a *different* prescription; therefore a session asked about twice is
the same delivery and a session that should be replaced is a new one. Nothing
here calls `PUT`.

> **Amended 2026-08-29 by decision 0021.** The middle step held only because
> nothing ever re-derived: once `prescribe` derives on every run, "a reissue is a
> different prescription" would make every run of the daily loop a new delivery.
> It is now true by construction rather than by luck — a derivation that produces
> the same `WorkoutShape` is not a reissue at all and writes nothing, so the
> delivery guard keyed on the prescription's identity stays sound. The
> conclusion is unchanged; what it rests on is not.

## Consequences

The routine id becomes a key rather than evidence. Open question 1 held that it
"identifies the routine, not the issue", and the corpus shows exactly that: of
the 8 landed workouts carrying a `routine_id`, 5 carry the same one. Under one
routine per issued prescription, an id names one session — which is what a later
prescribed↔performed correspondence needs, and the reason to record the reference
now even though nothing reads it yet.

Hevy's lack of a `DELETE` stops being a constraint we work around and becomes an
agreement with our own rule. Routines accumulate; a folder per programme is where
they accumulate, which is a rendering decision and lives with the renderer.

**The loss in rendering is declared, not silent.** The source has no effort field
on a routine set and no per-set rest, so both become notes — a change of medium
rather than a loss. What it genuinely cannot state comes back as `Unexpressed`
and is printed, for the reason `UnderivableSlot` is: the rest of the session
still arrived.

**One limitation is ours to declare and not to solve.** Starting a routine
pre-fills its sets with the prescribed numbers, so a session started and
abandoned without editing lands as a workout reporting our prescription as
performed. § 11's boundary blurs inside Hevy rather than inside us, and nothing
on this side can tell the two apart.

## Alternatives

**One routine rewritten in place.** Keeps the app tidy without folders, and is
what the operator did before. Rejected: it is what put five workouts behind one
id in the record, and it makes the prescribed side unmatchable exactly when the
negative gate needs an issued prescription to point at.

**Deleting superseded routines.** Rejected on the source's own terms — there is
no endpoint — but it would be wrong anyway. § 12 keeps a superseded prescription,
and a delivery that vanished would leave the record claiming a session was
delivered somewhere it no longer is.


**Delivering as part of `prescribe`.** Rejected. § 36 wants a source being
unavailable to degrade the system rather than fail it, and folding the two
together makes one exit code answer for a programme problem and a network
problem alike. Deriving a session advances a ladder; delivering one must not be
able to.
