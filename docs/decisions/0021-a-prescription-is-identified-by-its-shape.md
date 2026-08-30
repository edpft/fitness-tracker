# 0021 — A prescription is identified by its shape

**Date**: 2026-08-29

## Context

`prescribe` returned whatever was already issued for a date and did not derive
again unless `--reissue` was passed. The reasoning was § 12.1: a prescription is
authored data, and silently re-deriving a session the operator might be halfway
through is worse than making them ask.

It produced this, on the operator's store, on 2026-08-29:

```
$ fitness extract hevy.workouts && fitness normalise hevy.workouts
$ fitness prescribe
already issued for 2026-09-07 (Monday, light)
anchor 90kg (tested, from 2026-07-03, failed 95kg), week 5, history through 2026-08-28
…
already issued as prescription 3
```

Prescription 3 was issued on 2026-08-21 — a week before the 2026-08-28 session
it claims to have read, and under programme 4, which had been superseded twice
by then. The header line was not lying by accident: `history_through` was
re-read live on the already-issued path while every other field came from the
stored row, so the output advertised history the session had never seen.

The workflow it broke is the whole of the daily loop: extract, normalise,
prescribe, deliver. Step 3 is asked in the expectation that it reads what steps
1 and 2 just landed. It did not.

The obvious fix — make `--reissue` the default — is wrong in a way that took a
second pass to see. `deliver` guards against sending twice by asking whether
*this prescription id* has a delivery. A default that re-derives supersedes on
every run, every supersession is a new id, and every new id is a new delivery:
the operator would have got a fresh Hevy session every time they ran the daily
loop. The existing module doc said as much — "a reissue is a *different*
prescription, and so a session that should be replaced is a new delivery" — and
that was sound only while nothing ever re-derived.

## Decision

**Two prescriptions for a date are the same prescription when their
`WorkoutShape` is equal.** The shape is the session: the exercises, their order,
and the sets, reps, loads and durations. Everything else `PrescribedWorkout`
carries — `issued_at`, the programme version, the parameters and when they were
authored, the week, what the loads were derived from — is a fact *about* the
issuing, not part of the workout.

Three consequences follow, and the first is the one that was asked for:

1. **The ordinary run always derives.** There is no flag, and `Reissue` is gone
   from the port. The question it asked the caller — should this be derived
   again? — is answered by the record, and the caller was the party least able
   to answer it.

2. **An identical derivation writes nothing.** The prescription in force stays
   in force, with its identity and any delivery recorded against it. This is
   what keeps `deliver`'s guard sound without a second content check inside it,
   and it is the property that stops a duplicate reaching the operator's phone.

3. **A performed prescription is never re-derived**, whatever the shape would
   now be. Not because superseding would lose it — § 12 keeps every issue — but
   because `compare` reads the prescription in force for a date, and replacing
   it would leave the performance measured against a session that was never
   trained. What was prescribed is part of what the performance means.

A derivation that differs on a *drafted* or *published* prescription supersedes
it, as before.

**"In force" is where the third rule lives.** A date may hold several issues, and
the store answers with the performed one where there is one and the newest
otherwise. It is enforced in the query rather than in each use case because
`prescribe`, `deliver` and `compare` all ask the same question and any of them
could forget the rule. The window it closes is narrow and entirely reachable: a
session is delivered, a re-derivation supersedes it while it is still merely
published, and the operator then trains the routine already on their phone. The
newest row would be one nobody ever saw.

## Consequences

**A superseded published session is stranded, and is reported.** Withdrawal is
not built and Hevy publishes no `DELETE` for a routine (decision 0017), so
nothing here can remove the session already sent. `prescribe` names the stale
reference and says it needs removing by hand. That is the honest answer until
redelivery via `PUT` lands.

**A kept prescription keeps its original provenance.** Where the shape is
unchanged but the programme underneath it was superseded, the stored row still
names the older programme version. Accepted: the content is right, and writing a
new row to correct a note about where the content came from would be issuing a
second session to fix a footnote.

**The four outcomes are reported, not collapsed into a flag.** `Issuance` is
`Issued`, `Unchanged`, `Superseded` or `Performed`. `freshly_issued: bool` could
not distinguish the two that matter most: "the record moved and so did your
session" and "the record moved and your session did not".

**`--reissue` is removed rather than kept as an override.** With the ordinary
run deriving and a performed session immutable, the flag had no case left to
express. A flag whose only reachable effect is a refusal is a worse interface
than no flag.

## Alternatives considered

**Compare every field but `issued_at`.** The first shape of this, and wrong for
the reason the whole decision turns on: it defines a workout by subtracting one
field from the record of its issuing, rather than by naming what a workout is.
It would have made a re-authored parameter set or a superseded programme into a
redelivery, which is exactly the duplicate this exists to prevent.

**Keep `--reissue` for published prescriptions**, so the default could never
strand a session at a destination. Rejected: it makes the common case — a
drafted session that simply needs re-deriving — pay for the rare one, and the
operator being told plainly which reference is stale is more useful than being
asked to authorise something they cannot evaluate in advance.

**Guard the duplicate in `deliver` instead**, by comparing the shape about to be
sent against the shape last delivered. It would work, but it leaves the store
accumulating a superseding row on every run of the daily loop, each one a
prescription that says it replaced its predecessor without changing anything.
The store should not record events that did not happen.
