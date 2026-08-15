# 0001 — Retraction is resolved at the normalised layer

**Date**: 2026-08-14
**Status**: Accepted
**Amends**: the constitution, § II.3 and § 10. Version 1.0.0 → 1.0.1.
**Raised by**: `specs/002-hevy-workout-normalisation`, question Q1.

## What was decided

A source event withdrawing a record it previously served leaves that record with
no normalised entity. The withdrawal is applied where it is read — at the
normalised layer — rather than being carried forward as a thing the canonical
layer has to interpret.

## Why the question arose

Hevy's feed is current-state rather than an event log, so a deletion replaces a
workout's row instead of adding one. One `deleted` record is already landed, for
a workout created and deleted between two extraction runs; it carries an id and
a `deleted_at` and no body at all.

The constitution as ratified did not obviously answer it. § II.3 said "each
normalised entity is a function of exactly one raw landing record, never of
two", § 10 put supersession at the canonical layer and said the earlier record
"remains in raw and normalised but is not current", and § 7 required each
derivation to be a function of the one below it. Read literally, those three
together made every available option violate one of them.

## The options, and why the others lost

**A workout-shaped entity carrying a deletion marker.** The workout type would
have to admit an empty, body-less form — weakening "a non-empty ordered sequence
of items" for every workout in order to accommodate one record in 164. It also
has the layer assert a workout exists in order to record that it does not.

**A tombstone entity, peer of the workout.** Preserves both rules as written and
was the obvious safe answer. Rejected because it defers the question rather than
answering it: every consumer of the normalised layer then has to know that a
workout may be shadowed by a tombstone elsewhere in the output, and forgetting
to check reads a withdrawn workout as live. The invariant is stated in prose and
enforced by nobody, which is the shape of thing § 24 exists to reject.

**Nothing at the normalised layer.** The deletion stays in raw and the canonical
layer reads it directly. Simplest here, at the cost of § 7's chain: the
canonical layer would be a function of the normalised layer *and* of raw.

## Why this is a clarification and not a new rule

The literal reading above is the wrong one, and seeing why is the substance of
the decision.

Each layer has a job. The normalised layer translates one source into our model;
the canonical layer merges sources into one entry per real-world event. § II.3's
per-record rule serves that division: what it forbids is *composing* an entity
out of several records, because reconciling accounts is the canonical layer's
work and doing it early decides — at a layer that can see one source — something
that needs every source in view.

A retraction composes nothing. It carries no content, contributes no value, and
can only remove, so what an entity says is still exactly what one landing record
said. And the entity the rule counts records against is the workout that
happened in the gym, not the serving of it: a source's `updated` and `deleted`
records for one workout are two versions of one entity rather than two entities.
Applying a withdrawal is choosing among versions of a single thing, which is
what the layer is already for.

That is also what separates retraction from supersession, and why § 10 keeps the
latter where it is. Supersession names a replacement, so resolving it means
preferring one account of what happened over another, and § 10 reserves that for
the layer that can see them all. Retraction names no replacement. There are not
two accounts, so nothing is decided early by acting on it.

Re-derivation is unaffected: the retraction is a landing record like any other,
raw keeps everything the source ever said, and deriving again over unchanged raw
reproduces the same absence.

Hence PATCH rather than MAJOR. The letter of § II.3 changes; its spirit is what
the amendment writes down.

## Consequences

- The normalised layer's output for a stream is not one entity per landing
  record. A record's entity may be absent because a later record withdrew it.
- A retraction naming a record never landed removes nothing, and is not an
  error. That is the case already in the corpus.
- "Every landing record produces an entity or a recorded refusal" becomes
  "an entity, a retraction it applied, or a recorded refusal". A withdrawal is
  not a refusal — nothing was rejected.
- The canonical layer, when built, sees withdrawn workouts as absent rather than
  as present-and-marked. It needs no vocabulary for deletion.
