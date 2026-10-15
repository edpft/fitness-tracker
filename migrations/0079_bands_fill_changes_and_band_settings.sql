CREATE TABLE equipment_fact_rebuilt (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    implement    TEXT
        CHECK (implement IN ('barbell', 'dumbbell', 'kettlebell', 'cable',
                             'machine', 'band', 'plate', 'sled', 'sandbag',
                             'medicine-ball', 'bodyweight')),
    maker        TEXT,
    model        TEXT,
    valid_from   TEXT,
    recorded_at  TEXT    NOT NULL,
    corrects     INTEGER UNIQUE REFERENCES equipment_fact_rebuilt(id),
    most_grams   INTEGER CHECK (most_grams > 0),
    CHECK ((maker IS NULL) = (model IS NULL)),
    CHECK ((implement IS NULL) != (maker IS NULL)),
    CHECK (implement IS NULL OR most_grams IS NULL)
) STRICT;

INSERT INTO equipment_fact_rebuilt (id, implement, valid_from, recorded_at, corrects)
SELECT id, implement, valid_from, recorded_at, corrects
FROM equipment_fact
WHERE implement IS NOT NULL;

CREATE TABLE equipment_fact_step_rebuilt (
    fact        INTEGER NOT NULL REFERENCES equipment_fact_rebuilt(id),
    band        INTEGER NOT NULL CHECK (band >= 0),
    from_grams  INTEGER NOT NULL CHECK (from_grams >= 0),
    size_grams  INTEGER NOT NULL CHECK (size_grams > 0),
    CHECK (band > 0 OR from_grams = 0),
    PRIMARY KEY (fact, band)
) STRICT, WITHOUT ROWID;

INSERT INTO equipment_fact_step_rebuilt (fact, band, from_grams, size_grams)
SELECT step.fact, step.band, step.from_grams, step.size_grams
FROM equipment_fact_step AS step
JOIN equipment_fact_rebuilt AS fact ON fact.id = step.fact;

DROP TABLE equipment_fact_level;
DROP TABLE equipment_fact_step;
DROP TABLE equipment_fact;
ALTER TABLE equipment_fact_rebuilt RENAME TO equipment_fact;
ALTER TABLE equipment_fact_step_rebuilt RENAME TO equipment_fact_step;

CREATE TABLE equipment_fact_band (
    fact          INTEGER NOT NULL REFERENCES equipment_fact(id),
    position      INTEGER NOT NULL CHECK (position >= 0),
    name          TEXT    NOT NULL,
    twice_grams   INTEGER NOT NULL CHECK (twice_grams > 0),
    PRIMARY KEY (fact, position),
    UNIQUE (fact, name)
) STRICT, WITHOUT ROWID;

CREATE TABLE equipment_fact_band_step (
    fact      INTEGER NOT NULL REFERENCES equipment_fact(id),
    step      INTEGER NOT NULL CHECK (step >= 0),
    part      INTEGER NOT NULL CHECK (part >= 0),
    band      TEXT    NOT NULL,
    PRIMARY KEY (fact, step, part),
    FOREIGN KEY (fact, band) REFERENCES equipment_fact_band(fact, name)
) STRICT, WITHOUT ROWID;

CREATE TABLE gym_slot_fill_change (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    mesocycle       INTEGER NOT NULL REFERENCES gym_mesocycle(id) ON DELETE CASCADE,
    slot            TEXT    NOT NULL,
    valid_from      TEXT    NOT NULL,
    recorded_at     TEXT    NOT NULL,
    exercise        TEXT    NOT NULL,
    assisted_maker  TEXT,
    assisted_model  TEXT,
    CHECK ((assisted_maker IS NULL) = (assisted_model IS NULL))
) STRICT;

ALTER TABLE prescribed_exercise ADD COLUMN assisted_maker TEXT;
ALTER TABLE prescribed_exercise ADD COLUMN assisted_model TEXT;
ALTER TABLE prescribed_exercise ADD COLUMN assisted_grams INTEGER;

CREATE TABLE prescribed_exercise_band (
    workout        INTEGER NOT NULL,
    item_position  INTEGER NOT NULL,
    position       INTEGER NOT NULL,
    part           INTEGER NOT NULL CHECK (part >= 0),
    band           TEXT    NOT NULL,
    PRIMARY KEY (workout, item_position, position, part),
    FOREIGN KEY (workout, item_position, position)
        REFERENCES prescribed_exercise(workout, item_position, position)
) STRICT, WITHOUT ROWID;

ALTER TABLE correction ADD COLUMN assisted INTEGER NOT NULL DEFAULT 0
    CHECK (assisted IN (0, 1));
