CREATE TABLE withings_ecg_landing (
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

CREATE INDEX withings_ecg_landing_latest
    ON withings_ecg_landing (source_record_id, id DESC);

CREATE TRIGGER withings_ecg_landing_is_append_only_update
BEFORE UPDATE ON withings_ecg_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER withings_ecg_landing_is_append_only_delete
BEFORE DELETE ON withings_ecg_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE withings_ecg_signal_landing (
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

CREATE INDEX withings_ecg_signal_landing_latest
    ON withings_ecg_signal_landing (source_record_id, id DESC);

CREATE TRIGGER withings_ecg_signal_landing_is_append_only_update
BEFORE UPDATE ON withings_ecg_signal_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER withings_ecg_signal_landing_is_append_only_delete
BEFORE DELETE ON withings_ecg_signal_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;
