-- `peloton.workouts` gets a landing table of its own.
--
-- **One table per stream, and this is the second one.** Nothing here is shared
-- with `hevy_workout_landing`: the columns are the same because what every
-- landing record carries is the same whatever served it (§ II.1), not because
-- one table could have held both. A `source` column would be the shape that
-- lets two streams' resumption points and run locks drift into one.
--
-- **No normalised layer yet.** This lands raw and stops there; the performed
-- cycling entity and its translator are their own piece of work (#56). Raw
-- exists to be re-derived from, so landing first and deriving later is the
-- order § II.1 is built for.

CREATE TABLE peloton_workout_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    -- As on Hevy's table: the table name carries the source, and `endpoint`
    -- stays because the same workout can arrive from the list or from a
    -- direct fetch of one.
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    -- Verbatim from the source. Peloton serves a list rather than a change
    -- feed and so never says 'deleted'; every record here is 'updated', and
    -- a workout that disappears simply stops being served.
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX peloton_workout_landing_latest
    ON peloton_workout_landing (source_record_id, id DESC);

-- Its own pair of triggers, because a trigger is bound to a table. This is the
-- cost of one table per stream, and it is the cost worth paying.
CREATE TRIGGER peloton_workout_landing_is_append_only_update
BEFORE UPDATE ON peloton_workout_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER peloton_workout_landing_is_append_only_delete
BEFORE DELETE ON peloton_workout_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;
