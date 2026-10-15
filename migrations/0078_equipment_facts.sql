CREATE TABLE equipment_fact (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    implement    TEXT
        CHECK (implement IN ('barbell', 'dumbbell', 'kettlebell', 'cable',
                             'machine', 'band', 'plate', 'sled', 'sandbag',
                             'medicine-ball', 'bodyweight')),
    specific     TEXT,
    valid_from   TEXT,
    recorded_at  TEXT    NOT NULL,
    corrects     INTEGER UNIQUE REFERENCES equipment_fact(id),
    most_grams   INTEGER CHECK (most_grams > 0),
    CHECK ((implement IS NULL) != (specific IS NULL)),
    CHECK (implement IS NULL OR most_grams IS NULL)
) STRICT;

CREATE TABLE equipment_fact_step (
    fact        INTEGER NOT NULL REFERENCES equipment_fact(id),
    band        INTEGER NOT NULL CHECK (band >= 0),
    from_grams  INTEGER NOT NULL CHECK (from_grams >= 0),
    size_grams  INTEGER NOT NULL CHECK (size_grams > 0),
    CHECK (band > 0 OR from_grams = 0),
    PRIMARY KEY (fact, band)
) STRICT, WITHOUT ROWID;

CREATE TABLE equipment_fact_level (
    fact      INTEGER NOT NULL REFERENCES equipment_fact(id),
    position  INTEGER NOT NULL CHECK (position >= 0),
    name      TEXT    NOT NULL,
    PRIMARY KEY (fact, position),
    UNIQUE (fact, name)
) STRICT, WITHOUT ROWID;

INSERT INTO equipment_fact (implement, valid_from, recorded_at)
SELECT DISTINCT implement, NULL, parameters_authored_at
FROM generation_load_scale
WHERE parameters_authored_at = (SELECT max(parameters_authored_at) FROM generation_load_scale);

INSERT INTO equipment_fact_step (fact, band, from_grams, size_grams)
SELECT fact.id, scale.band, scale.from_grams, scale.size_grams
FROM generation_load_scale AS scale
JOIN equipment_fact AS fact ON fact.implement = scale.implement
WHERE scale.parameters_authored_at = (SELECT max(parameters_authored_at) FROM generation_load_scale);
