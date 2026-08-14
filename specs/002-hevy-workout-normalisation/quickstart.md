# Quickstart: validating Hevy workout normalisation

Phase 1. Every scenario below is a port-boundary integration test (§ 29) and
every one of them is also runnable by hand. The fixture is the landed corpus —
164 records already in the store, not re-fetched (spec, Assumptions).

## Prerequisites

```bash
nix develop                    # the toolchain, sqlite3, sqlx-cli
export FITNESS_TRACKER_DATABASE=local.db
export FITNESS_TRACKER_TIMEZONE=Europe/London
```

No credential. Nothing here contacts Hevy, which is the point of the layer.

## Run it

```bash
cargo nextest run              # the suite
fitness normalise hevy.workouts
fitness refusals hevy.workouts
fitness status hevy.workouts
```

## The suite

`cargo nextest run` inside `nix develop` is the inner loop; `nix flake check` is
the gate and is what CI runs.

Tests return `()` and assert by panicking, because `panic_in_result_fn` is
forbidden and `clippy.toml` allows panics in tests for exactly this. Fixture
builders are free functions, so they return `Result` and the test unwraps at the
call site — the test exemptions do not reach them.

The corpus is committed at
`crates/infrastructure/tests/fixtures/hevy-workouts.jsonl` — 164 records exported
verbatim — so the suite runs on a machine that has never talked to Hevy. The
suites that assert translation drive the use case with in-memory ports; the one
that asserts the store lands the fixture into a temporary SQLite file per test.

They live in `infrastructure` rather than `application` because they need the
Hevy translator, and `application` may not depend on the ring above it.

---

## Scenario 1 — The corpus translates to the model of record's figures

*User story 1. FR-001, FR-006 to FR-016. SC-001, SC-003.*

Derive over the 164 records and count.

| Assertion | Expected |
| ---: | --- |
| workouts written | 163 |
| exercise entries resolving through the mapping | 1,135 |
| performed exercises in the output | 1,122 |
| sets translated | 3,755 of 3,779 |
| well-formed groupings | 334 of 336 |
| supersets in the output | 328 |
| distinct template ids resolved | 134 of 134 |
| unmapped ids | 0 |

By hand:

```sql
SELECT (SELECT count(*) FROM gym_workout)         AS workouts,
       (SELECT count(*) FROM performed_exercise)  AS entries,
       (SELECT count(*) FROM performed_set)       AS sets;
```

## Scenario 2 — Re-derivation is identical

*FR-004, FR-028. SC-004.*

Derive, snapshot every normalised table and the refusal set, derive again, and
compare. Then delete the normalised layer entirely, derive a third time, and
compare against the first snapshot. No request is made to any source in any of
the three — asserted by wiring a translator whose source port does not exist, so
a request could not compile.

Then reverse the landing records and derive again: the result is equal. That is
FR-028, and it is what distinguishes an absorbing retraction from latest-wins.

## Scenario 3 — Identity comes from the mapping, not the label

*FR-016. SC-009.*

- `Pull Up` (97 sets), `Pull Up (Assisted)` (159) and `Pull Up (Band)` (3)
  resolve to one exercise; the assisted loads are negative and the plain ones
  are `Relative(0)`.
- `Chest Dip` (84) and `Chest Dip (Assisted)` (277) likewise.
- Template `DDB29047` resolves identically under both titles it has carried.
- Both `Overhead Squat` template ids resolve to one exercise.

## Scenario 4 — Exactly seven zeros refuse

*User story 2. FR-010. SC-002.* **The mapping's specification.**

93 sets carry a zero load. Assert that exactly 7 refuse with
`ZeroOnAbsoluteLoad`, that they sit on `Good Morning (Barbell)` (1), `Overhead
Squat` (2), `Romanian Deadlift (Barbell)` (2) and `Snatch-Grip Behind The Neck
Press` (2), and that the other 86 translate as `Relative`.

This fails until all 134 load interpretations are right, and it fails in both
directions — an eighth refusal means a bodyweight movement was called absolute,
a sixth means the reverse. It is written before the mapping exists (§ 31).

## Scenario 5 — The refusals are exactly the named set

*User story 2. FR-021 to FR-024. SC-002, SC-007.*

| Reason | Kind | Count |
| --- | --- | ---: |
| `ZeroOnAbsoluteLoad` | wrong data | 7 |
| `BandResistance` | declared limitation | 16 |
| `ZeroReps` | unmodelled | 1 |
| `NonContiguousSuperset` | wrong data | 1 |
| `SingleMemberSuperset` | wrong data | 1 |
| anything else | — | 0 |

24 sets and 2 supersets, and **no refusal outside that set**. Each names the
landing record, the position within it and a reason that distinguishes the three
kinds — asserted as a query over `RefusalReason`, never over rendered text.

The two malformed groupings are named: workout `b6995e63` has members at
positions 3 and 5 either side of a non-member; workout `3f9e9a6a` has a
single-member group at position 5. Both fail the definition, and in both cases
the member exercises still translate as ordinary items in their recorded order —
the workout is not lost to a bad grouping.

## Scenario 6 — Absence stays absent

*FR-011, FR-013. Spec's "Absence is absence".*

- 2,413 of the 3,755 translated sets carry an intensity and the rest have
  `None` — not zero, and not carried forward from a neighbouring set. The corpus
  records 2,415; two of them sit on sets that refused.
- 359 sets are warm-ups. The corpus tags 361; two are the Romanian deadlift's
  empty-bar zeros, which refuse.
- `rest_after` is `None` on all 3,755 translated sets. This source records none
  and none is invented from the `routine_id` on the 8 workouts that carry one
  (§ 11).
- No workout carries a title, description or note. Raw still holds them.

## Scenario 7 — Wall clock survives both switchovers

*FR-019, FR-020. SC-006.*

A workout stamped `2026-08-10T18:01:57+00:00` and one stamped in December at
19:00 UTC read back at the same local hour under `Europe/London`. No normalised
timestamp lacks a zone — which is a fact about the type, so the test that could
fail is the one asserting the wall clock, not the one asserting the zone exists.

Derive under a second zone and confirm the instants are unchanged and the wall
clocks move together. Then run with `FITNESS_TRACKER_TIMEZONE` unset and confirm the
command refuses rather than guessing (D4).

## Scenario 8 — A withdrawn workout is absent

*User story 3. FR-025 to FR-028. SC-010.*

- The corpus's single `deleted` record names a workout never landed. It
  withdraws nothing, derivation succeeds, 163 workouts result, and it appears in
  **no** refusal.
- Synthetically: land an `updated` record for that identifier and derive. 163
  workouts still — the one the retraction names is the one absent.
- Land the same two in the opposite order and derive. Same result.
- Every other workout is byte-identical to its counterpart in scenario 1.

## Scenario 9 — An unmapped identifier stops the run

*FR-017, FR-024's exception. SC-008.*

Synthesise a record carrying a template id the mapping does not cover. The run
fails, names the id, and writes nothing — the previous derivation is left
standing rather than half-replaced, which is D6's single transaction doing its
job. No workout containing the id translates around the gap.

## Scenario 10 — Every record is accounted for

*FR-005. SC-005.*

`records_read` equals `workouts_written` plus `workouts_withdrawn` plus
`retractions_applied` plus `records_refused`. Every landing record has exactly
one outcome: it became a workout that stands, a workout a retraction later
withdrew, a retraction of its own, or a refusal. Assert over the corpus that the
sum reconciles and that no landing record id appears both as a workout and as a
record-level refusal.

`workouts_withdrawn` is separate from `retractions_applied` because neither
implies the other — the corpus's one retraction names a workout never landed, so
it withdraws nothing.

## Scenario 11 — Supersession is not resolved here

*FR-001, US1 scenario 7.*

Synthetic, because the corpus has 164 distinct ids and no re-serve. Two
`updated` records sharing a source record id produce two workouts; both are
stored, neither is marked current, and neither is removed. Which is current is
§ 10's question and is not asked here.

## Scenario 12 — Derivation and extraction do not contend

*Spec's edge case.*

Run `normalise` while `extract` holds the run lock: both succeed. Derivation
takes no extraction lock and does not advance the resumption point, and a record
landed after derivation began is simply picked up by the next one.

---

## What is not tested here

The canonical layer, the Session, every metric, the edit overlay, prescription,
and every other source. None of it exists, and a test for it would be a test of
this feature's guesses about it.
