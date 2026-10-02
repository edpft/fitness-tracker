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
   | the prescription in force, rendered as it is now | already delivered; the destination hears nothing |
   | the prescription in force, rendered otherwise | `PUT` into that reference; the place keeps its occupant |
   | a superseded prescription | `PUT` into that reference; the place changes hands |
   | a performed prescription | already delivered; the destination hears nothing |

   The second and third rows were one row until 2026-10-02 — see the amendment
   below.

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
under that rather than the mechanism. (Amended: the first of those is no longer
true on its own — see below.)

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

## Amended 2026-10-02

**The place's occupant being unchanged does not mean what is in it is current.**
That is what the row "the prescription in force | already delivered; the
destination hears nothing" assumed, and it reads the *prescription* being
unchanged as the *routine* being up to date. The two part company the moment a
rendering is corrected.

`05 Heavy` for 2026-10-02 is the case (#343). Five of the front squat's eight
sets reached the phone showing no repetitions in the app's routine view, which
is the lift the session exists for; #342 fixed the rendering the next day and
could not reach the routine. `prescribe` re-derived the identical session and
issued nothing, so `deliver` found the place held by the prescription in force
and sent nothing. Hevy publishes no `DELETE`, so the operator's only remedy was
to type the corrected session in by hand. The operator: *"To me, it is a bug,
the delivered routine isn't useable in it's current state."*

**How bad that instance was is narrower than #341 and #343 say**, and the
operator established it on 2026-10-02, after this record was first amended: the
blanks are in the *routine* view, whose column is headed "rep range", and the
*workout* view fills every fixed count in. So the session was trainable all
along and what was lost was reading it beforehand. It changes nothing here. The
mechanism is not about one broken routine: a corrected rendering could not reach
*any* delivered session, whatever the correction was for, and that is what the
digest fixes.

**So a delivery records what was rendered, and the comparison is on that.** A
`RenderingDigest` — SHA-256 over the body the destination would send — sits
beside the reference in `prescription_delivery`, and `deliver` asks the
destination for the digest of what it renders now. Equal, and nothing is sent;
different, and the session is `PUT` into the place it already holds. The
occupant does not change, so this is neither a first delivery nor a hand-over:
`Placed::Replaced { superseding: None }`, and the store restates the row rather
than moving it.

The alternative was `deliver --replace`, a flag the operator types when they
notice a routine is wrong. Rejected because `fitness gym next` is the command
actually run, so its flag-free behaviour has to be the right one — a fix that
depends on spotting a bad routine does not close "a broken rendering stays on
the phone". The operator chose the digest and said what it is for: *"we only
want to replace the previously delivered routine because there was an issue with
rendering, if nothing has changed in content or form, then this should be a no
op, already delivered."*

**What the decision itself still says is untouched.** A reference names a place
rather than a delivered prescription; a date has at most one occupant per
destination; the hand-over is a delete and an insert so the trigger sees it.
Only the question asked before sending has grown a second half.

**Three details follow from it.**

A **null rendering is stale, not current.** Every routine delivered before this
column existed holds one — including the broken `05 Heavy` the issue was filed
for, which reading null as "current" would leave permanently unreachable. The
cost of the other reading is one `PUT` of a session that may already have been
right.

A **performed prescription is reachable now**, where the table above called it
unreachable, because the digest no longer stops at the occupant's identity. The
answer does not change: § 12 pins what has been performed, and a rendering fix
arriving after the work was done has nothing to correct. `deliver` declines to
send rather than leaving the trigger to abort, so the reason can be stated
rather than raised as a store error — and `occupying` answers whether a workout
names the place in the same query that answers who holds it.

**The folder is outside the digest.** Resolving one creates it when it is
missing, so including it would mean a request on every run that had nothing to
send. The digest answers what a routine says; where it is filed is the
reference's business.
