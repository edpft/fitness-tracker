-- no-transaction
PRAGMA foreign_keys = OFF;

CREATE TABLE correction_rebuilt (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    stream              TEXT NOT NULL,
    asserted_at         TEXT NOT NULL,
    reason              TEXT NOT NULL,
    exercise            TEXT,
    recorded_reps       INTEGER CHECK (recorded_reps IS NULL OR recorded_reps > 0),
    recorded_load_grams INTEGER CHECK (recorded_load_grams IS NULL OR recorded_load_grams >= 0),
    reps                INTEGER CHECK (reps IS NULL OR reps > 0),
    load                TEXT    CHECK (load IS NULL OR load IN ('nothing', 'mass', 'assistance')),
    load_grams          INTEGER CHECK (load_grams IS NULL OR load_grams >= 0),
    effort              TEXT    CHECK (effort IS NULL
                                       OR effort IN ('0', '0-1', '1', '1-2', '2', '2-3', '3', '4+')),
    CHECK ((exercise IS NOT NULL) + (recorded_reps IS NOT NULL) = 1),
    CHECK (exercise IS NULL OR (reps IS NULL AND load IS NULL AND effort IS NULL)),
    CHECK (recorded_reps IS NULL OR reps IS NOT NULL OR load IS NOT NULL OR effort IS NOT NULL),
    CHECK (recorded_load_grams IS NULL OR recorded_reps IS NOT NULL),
    CHECK ((coalesce(load, 'nothing') IN ('mass', 'assistance')) = (load_grams IS NOT NULL))
) STRICT;

INSERT INTO correction_rebuilt (
    id, stream, asserted_at, reason, exercise,
    recorded_reps, recorded_load_grams, reps, load, load_grams, effort
)
SELECT
    id, stream, asserted_at, reason, exercise,
    recorded_reps, recorded_load_grams, reps,
    CASE
        WHEN reps IS NULL THEN NULL
        WHEN load_grams IS NULL THEN 'nothing'
        WHEN assisted = 1 THEN 'assistance'
        ELSE 'mass'
    END,
    load_grams, NULL
FROM correction;

DROP TABLE correction;
ALTER TABLE correction_rebuilt RENAME TO correction;

CREATE INDEX correction_by_stream ON correction (stream);

PRAGMA foreign_keys = ON;
