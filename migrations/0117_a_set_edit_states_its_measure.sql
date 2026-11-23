-- no-transaction
PRAGMA foreign_keys = OFF;

CREATE TABLE canonical_gym_set_edit_rebuilt (
    edit             INTEGER PRIMARY KEY REFERENCES canonical_gym_edit(id) ON DELETE CASCADE,
    canonical_set    INTEGER NOT NULL REFERENCES canonical_gym_set_identity(id),
    reps             INTEGER CHECK (reps IS NULL OR reps > 0),
    duration_seconds INTEGER CHECK (duration_seconds IS NULL OR duration_seconds > 0),
    distance_mm      INTEGER CHECK (distance_mm IS NULL OR distance_mm > 0),
    load_kind        TEXT    CHECK (load_kind IN ('absolute', 'relative')),
    load_grams       INTEGER,
    rir              TEXT,
    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK (load_kind IS NOT 'absolute' OR load_grams >= 0),
    CHECK ((reps IS NOT NULL) + (duration_seconds IS NOT NULL) + (distance_mm IS NOT NULL) <= 1),
    CHECK (reps IS NOT NULL OR duration_seconds IS NOT NULL OR distance_mm IS NOT NULL
           OR load_kind IS NOT NULL OR rir IS NOT NULL)
) STRICT;

INSERT INTO canonical_gym_set_edit_rebuilt (edit, canonical_set, reps, load_kind, load_grams, rir)
SELECT edit, canonical_set, reps, load_kind, load_grams, rir FROM canonical_gym_set_edit;

DROP TABLE canonical_gym_set_edit;
ALTER TABLE canonical_gym_set_edit_rebuilt RENAME TO canonical_gym_set_edit;

PRAGMA foreign_keys = ON;
