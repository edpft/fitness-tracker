# Quickstart: validating Hevy workout extraction

How to prove the feature works end to end. Every acceptance scenario and every
success criterion in [spec.md](./spec.md) has a check below.

Scenarios 1–6 and SC-001…SC-004 run offline against a stub — they are the
suite, and they gate merge. The live section is a one-time confirmation against
the real account, run by hand.

## Prerequisites

```bash
nix develop            # or `direnv allow` once
```

The live section additionally needs a Hevy Pro API key from
<https://hevy.com/settings?developer>, in an untracked `.env`:

```bash
HEVY_API_KEY=<uuid>
FITNESS_TRACKER_DATABASE=./local.db
```

`.env` is gitignored and the `secrets` flake check scans for leaked credentials.
Never pass the key as a flag — see [contracts/cli.md](./contracts/cli.md).

## The gate

```bash
nix flake check        # everything CI runs
cargo nextest run      # fast inner loop, inside the dev shell
```

`nix flake check` is the merge gate. It builds every crate, runs clippy with
warnings denied, runs the suite, and runs `cargo audit`, `cargo deny`,
`gitleaks` and the architecture and workspace-membership checks. Nothing below
requires network access or a credential.

## Scenario coverage

Integration tests at the port boundaries (§ VII.29), driving the use case
through a stubbed `WorkoutEventSource` and a temporary SQLite file. Port and
error definitions are in [contracts/ports.md](./contracts/ports.md); the
response shapes replayed are in
[contracts/hevy-workout-events.md](./contracts/hevy-workout-events.md).

| Scenario | Setup | Assertion |
| --- | --- | --- |
| **1** First run | Stub serves 17 pages, 164 events | Every workout landed; `records_landed == 164`; watermark = newest event time |
| **2** Repeat, unchanged | Run twice against the same stub | Second run lands 0 records; total unchanged; run succeeds (**SC-002**) |
| **3** Workout edited | Stub re-serves one workout with a changed body | A second record for that id; the first byte-identical and still retrievable |
| **4** Workout deleted | Stub serves a `deleted` event for a landed workout | A record with kind `deleted`; the earlier record present and unaltered |
| **5** Interrupted run | Stub fails on page 9 of 17 | Run fails; pages 1–8 durable; watermark unmoved. Rerun against a healthy stub reaches the same state as one clean run (**SC-004**) |
| **6** Operator re-fetch | Land fully, `reset`, run again | Zero new records — identical payloads land nothing. Change one payload first: exactly one new record |
| **7** Source unavailable | Stub returns connection errors | Exit `1`; zero landing records; watermark unmoved; `status` still answers (§ 36) |

### Edge cases

| Case | Assertion |
| --- | --- |
| Delete for a workout never landed | Landed anyway, kind `deleted`. **Present in the real account** — the one `deleted` event's id appears in no `updated` event |
| Multiple edits between runs | Every distinct payload served is landed, in the order served |
| Empty account | Run succeeds, lands nothing, reports `0 events seen`. Not a failure (**FR-011**) |
| **Empty response uses the `workouts` key** | Zero events and a *successful* run — not a parse error. See below |
| Concurrent invocation | Second exits `2` immediately; no records; watermark unmoved (**FR-010**) |
| Unrecognised event kind | Landed with the kind recorded verbatim |
| Event with no timestamp | Landed with a null event time; contributes nothing to the watermark (§ 37) |

### The one that will bite

The empty response is `{"page":1,"page_count":1,"workouts":[]}` — key
`workouts`, not `events`, contradicting the published schema. It is what every
caught-up run receives, so a strict parser passes the first run and fails every
run after it. Test it directly:

```bash
cargo nextest run -E 'test(empty_page)'
```

## Manual validation against the live account

One-time, and the only part that touches Hevy. It is what confirms SC-001 and
SC-005; the offline suite cannot.

```bash
export $(grep -v '^#' .env | xargs)

fitness status                      # never run
fitness extract hevy                # ~17 requests, ~10s
fitness status                      # last succeeded: now
```

### SC-001 — completeness

Compare what was landed against the count Hevy reports independently:

```bash
curl -s -H "api-key: $HEVY_API_KEY" https://api.hevyapp.com/v1/workouts/count

sqlite3 local.db "
  SELECT COUNT(*) FROM (
    SELECT event_kind,
           ROW_NUMBER() OVER (
             PARTITION BY source_record_id ORDER BY id DESC
           ) AS recency
    FROM hevy_workout_landing
    -- no source filter: the table is Hevy workouts
  ) WHERE recency = 1 AND event_kind = 'updated';"
```

The two must be equal — 163 at the time of writing.

**Count workouts whose *most recent* record is an update.** A first run lands
164 distinct ids: 163 live workouts plus one that exists solely as a deletion.
That extra id is correctly landed and correctly absent from `workout_count`, so
comparing all distinct landed ids fails a correct run.

Filtering on `event_kind = 'updated'` alone is not enough either. It is right
today only because the two id sets happen to be disjoint on a first run. Once a
workout that was landed as an update is later deleted, raw holds both records
for it — that is what append-only means — and counting by kind would keep
counting it as live forever. The recency form above holds at any point in the
account's life.

This is a validation probe, not raw logic: § 10 resolves supersession at the
canonical layer, and nothing in the application reads raw this way. SC-001 in
the spec does not yet carry the qualifier; see
[research.md](./research.md) for the agreed wording.

### Deferred — deletion of a landed workout

The one live behaviour not yet confirmed. The evidence says a deletion
*replaces* the workout's row in the feed rather than adding one alongside it
(research.md), so deleting a workout should yield 162 `updated` + 2 `deleted`,
still 164 events, with `workout_count` falling to 162.

Run it on a disposable workout, never a real training record:

1. `POST /v1/workouts` a throwaway workout, or log one in the app.
2. `fitness extract hevy` — it lands as `updated`.
3. Delete it **in the Hevy app**. The API has no `DELETE` endpoint — `GET`,
   `POST` and `PUT` only — so this step cannot be scripted.
4. `fitness extract hevy` again.

Expected: one new landing record, kind `deleted`, for that id. The earlier
`updated` record is untouched and still retrievable (acceptance scenario 4,
SC-003), and the SC-001 query above still matches `workout_count` — which is
the assertion that the recency form buys and the kind filter would fail.

### SC-002 — idempotence

```bash
fitness extract hevy
sqlite3 local.db "SELECT COUNT(*) FROM hevy_workout_landing;"   # unchanged
```

Reports `0 events seen, 0 records landed`, and the resumption point is unmoved.

### SC-003 — immutability

Nothing is ever removed or altered. The store enforces this independently of the
application, so it can be checked directly:

```bash
sqlite3 local.db "UPDATE hevy_workout_landing SET payload = 'x' WHERE id = 1;"
# Error: raw landing is append-only (constitution II.1)

sqlite3 local.db "DELETE FROM hevy_workout_landing WHERE id = 1;"
# Error: raw landing is append-only (constitution II.1)
```

### SC-005 — no refetch needed downstream

Everything landed is a complete payload, so every derivation to come is a
function of the database alone:

```bash
sqlite3 local.db "
  SELECT COUNT(*) FROM hevy_workout_landing
  WHERE event_kind = 'updated' AND json_extract(payload, '$.workout.exercises') IS NULL;"
# 0 — no update record is missing its body
```

`json_extract` here is a validation probe, not how the system reads raw. Nothing
in the application interprets a payload at this layer (FR-002).

## Verifying FR-010 by hand

```bash
fitness extract hevy & fitness extract hevy; wait
```

One run proceeds; the other exits `2` immediately having landed nothing. Kill a
run mid-flight (`Ctrl-C`) and start another — it proceeds, because the advisory
lock is released by the kernel when the process dies. There is no stale lock to
clear.

## What "done" looks like

- [ ] `nix flake check` passes.
- [ ] Every scenario and edge case above is a test, and each fails before its
      implementation exists (§ 31, at the port boundary).
- [ ] Live run lands 163 `updated` ids matching `workout_count`, plus the one
      delete-only id.
- [ ] A second live run lands nothing.
- [ ] `UPDATE` and `DELETE` against `hevy_workout_landing` are refused by the store.
- [ ] `fitness status` reports the last success, and reports `never` cleanly
      before any run.
- [ ] No credential is committed; `gitleaks` passes.
