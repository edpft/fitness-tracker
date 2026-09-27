CREATE TABLE spreadsheet_file_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    path              TEXT    NOT NULL,
    modified_at       TEXT    NOT NULL,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX spreadsheet_file_landing_latest
    ON spreadsheet_file_landing (source_record_id, id DESC);

CREATE TRIGGER spreadsheet_file_landing_is_append_only_update
BEFORE UPDATE ON spreadsheet_file_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER spreadsheet_file_landing_is_append_only_delete
BEFORE DELETE ON spreadsheet_file_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;
