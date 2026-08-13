-- Raw landing for Hevy workouts, plus the operational tables that drive
-- extraction.
--
-- One landing table per source *and* entity type. Not one shared table with a
-- `source` column: what a source serves differs in shape and lifecycle, and a
-- discriminator column invites cross-source queries that the observation model
-- resolves at the canonical layer rather than here.
--
-- `extraction_run` and `resumption_point` are not observation data, so they are
-- single tables. Both are keyed by a landing stream — `hevy.workouts` —
-- because a resumption point belongs to one landing table rather than to a
-- source: collecting Hevy workouts and Hevy body measurements must never wait
-- on each other or share a position.

CREATE TABLE extraction_run (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    stream          TEXT    NOT NULL,
    started_at      TEXT    NOT NULL,
    finished_at     TEXT,
    outcome         TEXT    CHECK (outcome IN ('succeeded', 'failed')),
    events_seen     INTEGER,
    records_landed  INTEGER,
    failure_reason  TEXT,

    -- These mirror the RunOutcome sum type. The type makes the invalid
    -- combinations unrepresentable in Rust; these make them unrepresentable in
    -- the file, including to a writer that is not this program.
    --
    -- In flight is exactly "no outcome yet".
    CHECK ((outcome IS NULL) = (finished_at IS NULL)),
    -- A finished run always reports both counts. The difference between them
    -- is what distinguishes finding nothing new from finding nothing at all.
    CHECK (outcome IS NULL OR (events_seen IS NOT NULL AND records_landed IS NOT NULL)),
    -- A failure always says why; a success never does.
    CHECK ((outcome = 'failed') = (failure_reason IS NOT NULL))
);

-- The most recent successful extraction, in one indexed lookup.
CREATE INDEX extraction_run_succeeded
    ON extraction_run (stream, finished_at DESC)
    WHERE outcome = 'succeeded';

CREATE TABLE resumption_point (
    stream      TEXT PRIMARY KEY,
    watermark   TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE hevy_workout_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    -- No `source` column: the table name carries it, and it cannot vary within
    -- a table. `endpoint` stays because it genuinely can — the same workout can
    -- arrive from the events feed or from a direct fetch.
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    -- Verbatim from the source, including a kind we do not recognise.
    event_kind        TEXT    NOT NULL,
    -- Nullable on purpose: a source may serve an event without a timestamp,
    -- and substituting the fetch time would invent a fact and risk a
    -- resumption point that steps over events never seen.
    event_time        TEXT,
    -- BLOB rather than TEXT: the bytes as received, with no encoding round
    -- trip and nothing for SQLite to coerce.
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
);

-- The lookup made once per event: the most recent record for this workout.
-- No unique constraint on source_record_id — many records per workout over
-- time is the normal case, and is what append-only means.
CREATE INDEX hevy_workout_landing_latest
    ON hevy_workout_landing (source_record_id, id DESC);

-- Raw is never mutated, compacted or deleted.
--
-- Enforced here rather than by convention, because a convention is enforced
-- only against code that remembers it, while a trigger is enforced against
-- every writer including a stray sqlite3 session. Every future landing table
-- needs its own pair; that is the cost of one table per stream.
CREATE TRIGGER hevy_workout_landing_is_append_only_update
BEFORE UPDATE ON hevy_workout_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER hevy_workout_landing_is_append_only_delete
BEFORE DELETE ON hevy_workout_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;
