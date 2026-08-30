# 0022 — A delivery reference names a place at the destination

**Date**: 2026-08-30

**Revises**: 0017, whose "created, never updated" no longer holds.

## Context

Decision 0021 made re-derivation the ordinary case: `prescribe` derives on every
run and supersedes what is in force whenever the session differs. That was the
right answer to a prescription that had gone stale, and it left a hole one ring
out.

`deliver` asked *"has this prescription been delivered?"* — keyed on the
prescription's identity ([`reference_for`]). A corrected session is a different
prescription, so the answer was always no, so the correction was `POST`ed as a
routine of its own. Hevy publishes no `DELETE`, so the superseded session stayed
on the operator's phone and `prescribe` could do nothing but name it and
apologise:

```
issued as prescription 10, superseding 9
  prescription 9 was already delivered as 8e062cd4-…; that session is now out of
  date and needs removing at the destination
```

Two routines for one Monday, and the operator tidying up by hand. Their words on
seeing it: "*that's* why we need `PUT`".

Three fixes were considered and two rejected. **Having `deliver` refuse** when a
stranded sibling exists was rejected on the spot and for the right reason:
correcting a session is the thing being asked for, and a delivery that declines
to send the correction prevents the case it exists to serve. **`withdraw`** —
removing the old session and creating a new one — cannot be built against a
source with no `DELETE`.

`PUT /v1/routines/{routineId}` was in the pinned OpenAPI document all along.
0017's "nothing here calls `PUT`" was a choice, not a constraint; what 0017
observed about the source — no `DELETE`, and ids retired when a routine is
removed by hand — remains true and is unaffected.

## Decision

**A `DeliveryReference` names a place at a destination, not a delivered
prescription.** A date has at most one place per destination, and it is occupied
by whichever prescription for that date was most recently delivered into it.

Everything follows from that sentence:

1. **`deliver` asks about the date, not the prescription.** `occupying(date,
   destination)` replaces `reference_for(prescription, destination)` as the
   question that decides what happens:

   | what occupies the place | what happens |
   |---|---|
   | nothing | `POST`, and record the reference |
   | the prescription in force | already delivered; the destination hears nothing |
   | a superseded prescription | `PUT` into that reference; the place changes hands |
   | a performed prescription | unreachable — see below |

2. **The destination gains a second act.** `PrescriptionDestination::replace`
   beside `deliver`. `PutRoutinesRequestBody` and `PostRoutinesRequestBody` are
   identical field for field, down to the exercise and set schemas, so the two
   share one renderer; what differs is the route and the reply, which is a bare
   routine rather than a list containing one.

3. **The record moves rather than accumulating.** A hand-over deletes the
   superseded prescription's row and writes the successor's. One row per place
   means every join on a reference stays unambiguous — `state_of`, `fulfilling`
   and the trigger pinning a performed session all keep working untouched.

4. **The hand-over is a delete and an insert in one transaction, never an
   `UPDATE`.** `prescription_delivery_performed_is_not_deletable` is a
   `BEFORE DELETE` trigger, so routing the hand-over through a delete is what
   makes "a performed session is not replaced" hold in the schema. An `UPDATE
   ... SET prescription = ?` would slide straight past it.

5. **A routine the source no longer holds is refused, not recreated.** A 404 on
   the `PUT` means the operator deleted it by hand. `DeliveryError::Vanished`
   says so. Falling back to a `POST` would resolve a disagreement between the
   store and the app by destroying the evidence of it.

## Consequences

**The property 0017 was protecting is given up, and replaced.** A routine id no
longer names exactly one issued session — which is the thing 0017 wanted, and
the record shows why: of the 8 landed workouts carrying a routine id, 5 carry
the same one, because that routine was rewritten in place. What makes the
pairing sound now is not the id's uniqueness over time but the store's: exactly
one prescription holds a reference at any moment, because the place is handed
over rather than shared. A workout naming a reference names that prescription.

**The store stops recording that a superseded prescription was ever delivered.**
Accepted deliberately. § 12.1 calls a published prescription cheap and says
withdrawing it means removing the session at the destination — which is exactly
what the replacement did. "Prescription 9 was briefly on the phone" answers no
question anyone asks.

**The performed case is closed twice over.** Decision 0021 made a performed
prescription the one in force for its date, so `deliver` finds the place already
held by the session it is delivering and sends nothing. The trigger is the floor
under that rather than the mechanism.

**`prescribe`'s warning changes from a chore to an instruction.** The superseded
session is stale rather than stranded, and the line now reads "deliver to
replace it".

## Alternatives considered

**Accumulate the delivery rows**, keeping both prescriptions against one
reference and taking the latest by `delivered_at`. Keeps the history, at the
cost of teaching every join on a reference to resolve an ambiguity — including
the trigger, which would have to distinguish the current occupant from a former
one to know whether a delete was allowed. A record that makes four queries
harder to keep one fact nobody asks for is the wrong trade.

**A `destination_place` table**, keyed by destination and reference, pointing at
its current occupant, with `prescription_delivery` kept append-only as history.
The conceptually cleanest of the three and the largest. Worth revisiting only if
a second destination turns out to need a place model of its own; today it would
be two tables expressing what one row already says.
