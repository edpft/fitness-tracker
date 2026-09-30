CREATE TABLE exercise_correction (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    stream      TEXT NOT NULL,
    exercise    TEXT NOT NULL,
    asserted_at TEXT NOT NULL,
    reason      TEXT NOT NULL
) STRICT;

CREATE INDEX exercise_correction_by_stream ON exercise_correction (stream);

CREATE TABLE exercise_correction_term (
    correction       INTEGER NOT NULL REFERENCES exercise_correction(id) ON DELETE CASCADE,
    source_record_id TEXT    NOT NULL,
    term             TEXT    NOT NULL,
    PRIMARY KEY (correction, source_record_id, term)
) STRICT, WITHOUT ROWID;

CREATE INDEX exercise_correction_term_by_record
    ON exercise_correction_term (source_record_id, term);
