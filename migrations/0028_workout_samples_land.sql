-- `peloton.workout_samples` gets a landing table of its own.
--
-- **The third landing table, and the second stream from one source.** A source
-- serving a second kind of thing is a second stream: this resumes, runs and
-- locks independently of `peloton.workouts`, which is what lets a walk of the
-- graphs fail without costing the workout record its resumption point.
--
-- **Why it is not a wider workout payload.** The samples come from a different
-- endpoint — one request per workout against
-- `/api/workout/{id}/performance_graph` — and a landing record holds one
-- response as served. Combining two responses into one payload would be this
-- adapter composing a document the source never sent, which is the one thing
-- raw exists to prevent.
--
-- **What a graph carries**, verified against the operator's own account: a
-- stream per metric at 1 Hz, indexed by seconds since pedalling started rather
-- than by the class clock; totals; and averages. A cycling workout has five
-- streams — output, cadence, resistance, speed, heart rate — and every other
-- discipline has heart rate alone. None of that is interpreted here. It is
-- landed as bytes and § II.1 keeps it that way until something derives from it.
--
-- The columns are the other landing tables' because what every landing record
-- carries is the same whatever served it, not because one table could hold
-- them all.

CREATE TABLE peloton_workout_sample_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    -- The workout the graph belongs to, which is the same identifier
    -- `peloton_workout_landing` files its record under. Two streams naming one
    -- workout is what makes them joinable; it is not a foreign key, because raw
    -- does not depend on raw and either stream may be collected first.
    source_record_id  TEXT    NOT NULL,
    -- This source has no event kinds. A graph is served as it currently stands,
    -- so every record here is 'updated'.
    event_kind        TEXT    NOT NULL,
    -- The workout's own creation time, taken from the list that enumerated it.
    -- The graph itself states no time, and substituting the fetch clock would
    -- invent a fact and risk a resumption point stepping over unseen records.
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    -- Nullable for the reason given in 0027: a null reads as the payload's own
    -- digest. Nothing about a graph is known to be volatile — it is the
    -- operator's own performance rather than a class's public counters — so
    -- this is expected to equal `payload_digest` until something proves
    -- otherwise, and the machinery is here if it does.
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX peloton_workout_sample_landing_latest
    ON peloton_workout_sample_landing (source_record_id, id DESC);

CREATE TRIGGER peloton_workout_sample_landing_is_append_only_update
BEFORE UPDATE ON peloton_workout_sample_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER peloton_workout_sample_landing_is_append_only_delete
BEFORE DELETE ON peloton_workout_sample_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;
