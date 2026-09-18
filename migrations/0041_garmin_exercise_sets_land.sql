CREATE TABLE garmin_exercise_set_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_exercise_set_landing_latest
    ON garmin_exercise_set_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_exercise_set_landing_is_append_only_update
BEFORE UPDATE ON garmin_exercise_set_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_exercise_set_landing_is_append_only_delete
BEFORE DELETE ON garmin_exercise_set_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;
