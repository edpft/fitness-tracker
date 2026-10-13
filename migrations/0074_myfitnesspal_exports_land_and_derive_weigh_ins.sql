CREATE TABLE myfitnesspal_export_landing (
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

CREATE INDEX myfitnesspal_export_landing_latest
    ON myfitnesspal_export_landing (source_record_id, id DESC);

CREATE TRIGGER myfitnesspal_export_landing_is_append_only_update
BEFORE UPDATE ON myfitnesspal_export_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER myfitnesspal_export_landing_is_append_only_delete
BEFORE DELETE ON myfitnesspal_export_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE myfitnesspal_weigh_in (
    weigh_in           INTEGER PRIMARY KEY REFERENCES weigh_in(id),
    landing_record_id  INTEGER NOT NULL REFERENCES myfitnesspal_export_landing(id),
    sheet              TEXT    NOT NULL CHECK (length(sheet) > 0),
    cell               TEXT    NOT NULL CHECK (length(cell) > 0),
    entered_at_utc     TEXT    NOT NULL,
    UNIQUE (landing_record_id, sheet, cell)
) STRICT;
