DROP TABLE exercise_correction_term;

DROP TABLE exercise_correction;

CREATE TABLE correction (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    stream              TEXT NOT NULL,
    asserted_at         TEXT NOT NULL,
    reason              TEXT NOT NULL,
    exercise            TEXT,
    recorded_reps       INTEGER,
    recorded_load_grams INTEGER,
    reps                INTEGER,
    load_grams          INTEGER,
    CHECK ((exercise IS NOT NULL) + (reps IS NOT NULL) = 1),
    CHECK ((reps IS NULL) = (recorded_reps IS NULL)),
    CHECK (reps IS NULL OR reps > 0),
    CHECK (recorded_reps IS NULL OR recorded_reps > 0),
    CHECK (load_grams IS NULL OR reps IS NOT NULL),
    CHECK (recorded_load_grams IS NULL OR recorded_reps IS NOT NULL),
    CHECK (load_grams IS NULL OR load_grams >= 0),
    CHECK (recorded_load_grams IS NULL OR recorded_load_grams >= 0)
) STRICT;

CREATE INDEX correction_by_stream ON correction (stream);

CREATE TABLE correction_term (
    correction       INTEGER NOT NULL REFERENCES correction(id) ON DELETE CASCADE,
    source_record_id TEXT    NOT NULL,
    term             TEXT    NOT NULL,
    PRIMARY KEY (correction, source_record_id, term)
) STRICT, WITHOUT ROWID;

CREATE INDEX correction_term_by_record ON correction_term (source_record_id, term);
