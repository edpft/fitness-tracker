# Contract: operator command surface

Phase 1. Two new subcommands and one extended one. Built with clap's **builder**
API, never the derive macros — they generate `#[allow(clippy::unwrap_used)]`,
which is a compile error under a `forbid` lint (CLAUDE.md).

## `fitness normalise <stream>`

Derives the normalised layer for a stream from what raw already holds. Makes no
request to any source.

```text
fitness normalise hevy.workouts
```

**Environment**: `FITNESS_TIMEZONE` (IANA identifier, required — D4),
`FITNESS_DATABASE` as extraction already uses. No flag carries the zone: it is a
declared interpretive parameter, not a per-invocation choice, and an operator
who can pass it per run can produce two derivations that disagree.

**Output**:

```text
derived hevy.workouts
  records read       164
  workouts written   163
  retractions         1
  refusals           26
```

The four numbers must reconcile — records read equals workouts plus retractions
plus records that yielded nothing — which is SC-005 visible at the terminal.

**Exit codes**: `0` derived; `1` the store was unavailable, or an exercise
template id is unmapped. The second names the id and the record it appeared in,
because that is a defect in our vocabulary to go and fix (FR-017).

Refusals do **not** affect the exit code. A run that recorded 26 of them
succeeded — it found 26 things wrong with the data and said so, which is the
feature working.

## `fitness refusals <stream>`

Reads back what the last derivation would not accept (FR-023).

```text
fitness refusals hevy.workouts
```

**Output**: one line per refusal, grouped by kind, with the record, the position
within it and the reason:

```text
hevy.workouts — 26 refusals from the derivation of 2026-08-14T09:12:03Z

wrong data (9)
  b6995e63…  superset 1              members either side of a non-member
  3f9e9a6a…  superset 2              single member
  0a1c…      exercise 4              zero load on an absolute-load exercise
  …

declared limitation (16)
  …          exercise 2, set 0       band resistance is not modelled
  …

unmodelled (1)
  …          exercise 7, set 7       a set of zero reps
```

Grouped by `RefusalReason::kind()` rather than by record, because the operator's
question is "what do I need to fix, what am I living with, and what does the
model not hold yet" — three different actions, and the model of record says
telling them apart is the point of recording them at all.

**Exit codes**: `0` whether or not there are refusals. Nothing here is a failure.

## `fitness status <stream>` — extended

Gains a second section, so § 38's "staleness is observable" covers the
derivation as well as the extraction:

```text
hevy.workouts

extraction
  last success       2026-08-14T08:55:01Z
  records held       164
  resumption point   2026-08-10T19:29:47.199Z

normalisation
  last success       2026-08-14T09:12:03Z
  workouts           163
  refusals            26
  records behind      0
```

`records behind` is raw's record count minus what the last successful derivation
read. Non-zero means raw has moved since and the normalised layer is stale —
the one number that makes a forgotten `normalise` visible rather than silent.

## What is not added

- **No `--dry-run`.** A derivation is already re-runnable and replaces itself;
  a mode that computes and discards is a second path to keep correct.
- **No flag to skip refusals or to continue past an unmapped id.** The first is
  the feature; the second is a defect that must not be routed around
  (FR-017).
- **No scheduling.** Derivation is invoked, exactly as extraction is.
