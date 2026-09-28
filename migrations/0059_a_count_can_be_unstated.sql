PRAGMA defer_foreign_keys = ON;

UPDATE performed_exercise SET exercise = 'lu-raise' WHERE exercise = 'overhead-plate-raise';
UPDATE prescribed_exercise SET exercise = 'lu-raise' WHERE exercise = 'overhead-plate-raise';
UPDATE gym_slot_fill SET exercise = 'lu-raise' WHERE exercise = 'overhead-plate-raise';
UPDATE gym_mesocycle SET primary_exercise = 'lu-raise' WHERE primary_exercise = 'overhead-plate-raise';
UPDATE normalisation_refusal SET exercise = 'lu-raise' WHERE exercise = 'overhead-plate-raise';

CREATE TABLE performed_set_next (
    workout            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    exercise_position  INTEGER NOT NULL,
    position           INTEGER NOT NULL,
    load_kind          TEXT    CHECK (load_kind IN ('absolute', 'relative')),
    load_grams         INTEGER,
    outcome            TEXT    NOT NULL CHECK (outcome IN ('completed', 'failed')),
    reps               INTEGER,
    duration_seconds   INTEGER,
    distance_mm        INTEGER,
    rir                TEXT,
    set_kind           TEXT    NOT NULL CHECK (set_kind IN ('working', 'warmup')),
    rest_after_seconds INTEGER,
    sheet              TEXT,
    cell               TEXT,
    landing_record_id  INTEGER,
    PRIMARY KEY (workout, item_position, exercise_position, position),
    FOREIGN KEY (workout, item_position, exercise_position)
        REFERENCES performed_exercise(workout, item_position, position),
    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK ((sheet IS NULL) = (cell IS NULL)),
    CHECK (load_kind IS NOT 'absolute' OR load_grams >= 0),
    CHECK (outcome != 'failed'
           OR (reps IS NULL AND duration_seconds IS NULL AND distance_mm IS NULL)),
    CHECK (reps IS NULL OR (duration_seconds IS NULL AND distance_mm IS NULL)),
    CHECK (reps IS NULL OR reps > 0)
) STRICT, WITHOUT ROWID;

INSERT INTO performed_set_next
SELECT workout, item_position, exercise_position, position, load_kind, load_grams, outcome,
       reps, duration_seconds, distance_mm, rir, set_kind, rest_after_seconds, sheet, cell,
       landing_record_id
FROM performed_set;

DROP TABLE performed_set;
ALTER TABLE performed_set_next RENAME TO performed_set;
