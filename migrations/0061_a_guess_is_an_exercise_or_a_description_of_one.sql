CREATE TABLE measured_set_next (
    workout         INTEGER NOT NULL REFERENCES gym_workout(id),
    position        INTEGER NOT NULL,
    started_at_utc  TEXT    NOT NULL,
    zone            TEXT    NOT NULL,
    reps            INTEGER NOT NULL,
    load_kind       TEXT    CHECK (load_kind IN ('absolute', 'relative')),
    load_grams      INTEGER,
    guess_exercise  TEXT,
    guess_movement  TEXT,
    guess_implement TEXT,
    guess_from      TEXT    CHECK (guess_from IN ('proposed', 'from-its-run')),
    PRIMARY KEY (workout, position),
    CHECK (reps > 0),
    CHECK ((guess_from IS NULL)
           = (guess_exercise IS NULL AND guess_movement IS NULL)),
    CHECK (guess_exercise IS NULL OR guess_movement IS NULL),
    CHECK (guess_implement IS NULL OR guess_movement IS NOT NULL),
    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK (load_kind IS NOT 'absolute' OR load_grams >= 0)
) STRICT, WITHOUT ROWID;

INSERT INTO measured_set_next (
    workout, position, started_at_utc, zone, reps, load_kind, load_grams,
    guess_exercise, guess_movement, guess_implement, guess_from
)
SELECT workout, position, started_at_utc, zone, reps, load_kind, load_grams,
       guess, NULL, NULL, guess_from
FROM measured_set;

DROP TABLE measured_set;

ALTER TABLE measured_set_next RENAME TO measured_set;
