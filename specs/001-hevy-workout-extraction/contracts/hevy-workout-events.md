# Contract: Hevy workout events feed (consumed)

The external contract this feature depends on. Not ours — recorded so that a
change on Hevy's side is a failing test rather than a mystery.

Verified against the live API on **2026-08-11**. The document Hevy publishes is
pinned alongside as [`hevy-openapi.pinned.json`](./hevy-openapi.pinned.json);
where the two disagree, this file records what the API actually did.

## Endpoints used

| Endpoint | Used for |
| --- | --- |
| `GET /v1/workouts/events` | Collection. The only source of landing records |
| `GET /v1/workouts/count` | SC-001's independent count. Quickstart assertion only — never landed |

`GET /v1/workouts` is deliberately not used: it omits deletions (research.md).

## Request

```http
GET /v1/workouts/events?since=1970-01-01T00:00:00Z&page=1&pageSize=10
api-key: <uuid>
```

| Parameter | Constraint | Verified |
| --- | --- | --- |
| `api-key` (header) | Required. Hevy Pro accounts only | `401` body `InvalidApiKey`, plain text — **not JSON** |
| `since` | RFC 3339. Default `1970-01-01T00:00:00Z` | **Inclusive.** Filters on `updated_at` / `deleted_at` |
| `page` | ≥ 1, default 1 | Beyond `page_count` → `404` `{"error":"Page not found"}` |
| `pageSize` | 1–10, default 5 | > 10 → `400` `{"error":"pageSize must be less than or equal to 10"}` |

**Ordering is newest → oldest**, verified across all 164 events in the account.

## Response — populated

```jsonc
{
  "page": 1,
  "page_count": 17,
  "events": [
    {
      "type": "updated",
      "workout": {
        "id": "b459cba5-cd6d-463c-abd6-54f8eafcadcb",
        "title": "Morning Workout 💪",
        "description": "Pushed myself to the limit today!",
        "routine_id": "b459cba5-cd6d-463c-abd6-54f8eafcadcb",
        "start_time": "2021-09-14T12:00:00Z",
        "end_time": "2021-09-14T13:00:00Z",
        "created_at": "2021-09-14T13:00:00Z",
        "updated_at": "2021-09-14T13:00:00Z",
        "exercises": [
          {
            "index": 0,
            "title": "Bench Press (Barbell)",
            "notes": "Felt great",
            "exercise_template_id": "05293BCA",
            "superset_id": null,          // NB: `superset_id`, not `supersets_id`
            "sets": [
              {
                "index": 0,
                "type": "normal",
                "weight_kg": 100,
                "reps": 10,
                "distance_meters": null,
                "duration_seconds": null,
                "rpe": 9.5,
                "custom_metric": null
              }
            ]
          }
        ]
      }
    },
    {
      "type": "deleted",
      "id": "93d50b8d-f806-4042-959f-263dbb6a53f7",
      "deleted_at": "2025-11-05T20:02:27.905Z"
    }
  ]
}
```

Field values above are the published examples, not this account's data. Key
names and the null-versus-absent behaviour are as observed.

## Response — empty ⚠

**The empty result uses a different key.**

```jsonc
{"page": 1, "page_count": 1, "workouts": []}
```

Not `events`. Reproduced with `since=2099-01-01T00:00:00Z` and with any `since`
after the newest event. The published schema marks `events` as required, so a
deserialiser generated from it fails here.

This is the steady state — every repeat run once caught up returns exactly this
(acceptance scenario 2, FR-005, SC-002). **Required handling**: a response
carrying `workouts`, or carrying neither key, is *zero events*, not an error.
Both shapes deserialise to an empty page.

## Event kinds

| `type` | Identifier at | Event time | Notes |
| --- | --- | --- | --- |
| `updated` | `workout.id` | `workout.updated_at` | Carries the full body. Also how creation surfaces — there is no `created` kind |
| `deleted` | `id` | `deleted_at` | Id only. No body ever existed to land |
| anything else | `workout.id` or `id` | — | Landed verbatim with the kind recorded as-is (D12) |

**A delete-only workout exists in this account**: the one `deleted` event's id
appears in no `updated` event. The spec's "delete event for a workout that was
never landed" edge case is live on the first run.

### The feed carries one row per workout, not a log

The two id sets do not overlap, and the workout deleted in November 2025 shows
*only* as `deleted` despite certainly having been served as `updated` before
then. **Deletion replaces a workout's row; it does not add one.** The feed is
current state per workout — 164 rows for the 164 workouts the account has ever
held — and `since` filters on whichever timestamp the row currently bears.

Deleted rows persist indefinitely (this one for nine months), so a full
re-fetch always re-serves deletions.

Predicted, untested: deleting one more workout yields 162 `updated` + 2
`deleted`, still 164 events, `workout_count` 162. There is no `DELETE`
endpoint, so confirming it needs the Hevy app — deferred live check in
[quickstart.md](../quickstart.md).

## Observed characteristics

| Property | Value |
| --- | --- |
| Full history | 17 requests, ~10 s, 164 events for 163 workouts |
| Latency | 0.36–4.85 s per request, median ≈0.5 s |
| Rate limiting | None observed in 30 rapid requests; no `429` documented; no rate-limit headers |
| Server | Heroku / Express |

## Failure handling required

| Condition | Treatment |
| --- | --- |
| `429`, `5xx`, transport error | Retry with bounded exponential backoff and jitter; honour `Retry-After` if present. Exhausted → `SourceError::Unavailable` |
| `401` | Terminal. `SourceError::Unauthorised` — never retried |
| `400`, `404` | Terminal. `SourceError::Malformed` — a bug in our request, not a transient fault |
| Body is not JSON | `SourceError::Malformed`. Error bodies are not reliably JSON |

## What is pinned by test

Contract tests replay recorded responses against a local stub, so these hold
without touching the live account or a credential:

1. A populated page splits into one landing record per event, bytes unchanged.
2. **The empty `workouts` response yields zero events and a successful run.**
3. `since` is passed through unmodified from the stored watermark.
4. Pagination walks `1..=page_count` and stops — it never requests `page_count + 1`.
5. `401` is terminal; `429` and `5xx` retry then surface as `Unavailable`.
6. An unrecognised `type` is landed with its kind verbatim.
