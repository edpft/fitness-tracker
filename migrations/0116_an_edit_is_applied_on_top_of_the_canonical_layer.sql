DROP TABLE correction_term;
DROP TABLE correction;

CREATE TABLE canonical_gym_edit (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    asserted_at TEXT    NOT NULL,
    reason      TEXT    NOT NULL CHECK (trim(reason) != '')
) STRICT;

CREATE TABLE canonical_gym_exercise_edit (
    edit     INTEGER NOT NULL REFERENCES canonical_gym_edit(id) ON DELETE CASCADE,
    exercise INTEGER NOT NULL REFERENCES canonical_gym_exercise_identity(id),
    becomes  TEXT    NOT NULL,
    read     TEXT    NOT NULL,
    sets     TEXT    NOT NULL CHECK (json_valid(sets)),
    PRIMARY KEY (edit, exercise)
) STRICT, WITHOUT ROWID;

CREATE TABLE canonical_gym_set_edit (
    edit          INTEGER PRIMARY KEY REFERENCES canonical_gym_edit(id) ON DELETE CASCADE,
    canonical_set INTEGER NOT NULL REFERENCES canonical_gym_set_identity(id),
    reps          INTEGER CHECK (reps IS NULL OR reps > 0),
    load_kind     TEXT    CHECK (load_kind IN ('absolute', 'relative')),
    load_grams    INTEGER,
    rir           TEXT,
    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK (load_kind IS NOT 'absolute' OR load_grams >= 0),
    CHECK (reps IS NOT NULL OR load_kind IS NOT NULL OR rir IS NOT NULL)
) STRICT;

DELETE FROM canonical_gym_set;
DELETE FROM canonical_gym_exercise;
DELETE FROM canonical_gym_item;
DELETE FROM canonical_gym_session_heart_rate;
DELETE FROM canonical_gym_session;
DELETE FROM canonical_changed_day WHERE layer = 'gym';
DELETE FROM canonical_derivation WHERE layer = 'gym';

DROP TABLE canonical_gym_set;
DROP TABLE canonical_gym_exercise;

CREATE TABLE canonical_gym_exercise (
    id                 INTEGER NOT NULL UNIQUE REFERENCES canonical_gym_exercise_identity(id),
    session            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    position           INTEGER NOT NULL,
    measure            TEXT    NOT NULL CHECK (measure IN ('reps', 'duration', 'distance')),
    identified         TEXT    NOT NULL
        CHECK (identified IN ('recorded', 'proposed', 'from-its-run', 'undetermined')),
    exercise           TEXT,
    movement           TEXT,
    implement          TEXT,
    identified_normalised_session INTEGER REFERENCES gym_session(id),
    PRIMARY KEY (session, item_position, position),
    FOREIGN KEY (session, item_position) REFERENCES canonical_gym_item(session, position),
    CHECK (exercise IS NULL OR movement IS NULL),
    CHECK (implement IS NULL OR movement IS NOT NULL),
    CHECK ((identified = 'undetermined') = (exercise IS NULL AND movement IS NULL)),
    CHECK (measure = 'reps' OR identified = 'recorded'),
    CHECK (identified_normalised_session IS NOT NULL OR identified = 'recorded')
) STRICT, WITHOUT ROWID;

CREATE TABLE canonical_gym_set (
    id                 INTEGER NOT NULL UNIQUE REFERENCES canonical_gym_set_identity(id),
    sources            TEXT    NOT NULL CHECK (json_valid(sources)),
    session            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    exercise_position  INTEGER NOT NULL,
    position           INTEGER NOT NULL,
    outcome            TEXT    NOT NULL CHECK (outcome IN ('completed', 'failed')),
    reps               INTEGER,
    duration_seconds   INTEGER,
    distance_mm        INTEGER,
    outcome_normalised_session    INTEGER REFERENCES gym_session(id),
    load_kind          TEXT    CHECK (load_kind IN ('absolute', 'relative')),
    load_grams         INTEGER,
    load_normalised_session       INTEGER REFERENCES gym_session(id),
    began_at_utc       TEXT,
    began_zone         TEXT,
    began_normalised_session      INTEGER REFERENCES gym_session(id),
    rir                TEXT,
    rir_normalised_session        INTEGER REFERENCES gym_session(id),
    set_kind           TEXT    CHECK (set_kind IN ('working', 'warmup')),
    set_kind_normalised_session   INTEGER REFERENCES gym_session(id),
    rest_after_seconds INTEGER,
    rest_after_normalised_session INTEGER REFERENCES gym_session(id),
    PRIMARY KEY (session, item_position, exercise_position, position),
    FOREIGN KEY (session, item_position, exercise_position)
        REFERENCES canonical_gym_exercise(session, item_position, position),
    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK (load_kind IS NOT NULL OR load_normalised_session IS NULL),
    CHECK (load_kind IS NOT 'absolute' OR load_grams >= 0),
    CHECK ((began_at_utc IS NULL) = (began_zone IS NULL)),
    CHECK ((began_at_utc IS NULL) = (began_normalised_session IS NULL)),
    CHECK (rir IS NOT NULL OR rir_normalised_session IS NULL),
    CHECK ((set_kind IS NULL) = (set_kind_normalised_session IS NULL)),
    CHECK ((rest_after_seconds IS NULL) = (rest_after_normalised_session IS NULL)),
    CHECK (outcome != 'failed'
           OR (reps IS NULL AND duration_seconds IS NULL AND distance_mm IS NULL)),
    CHECK (reps IS NULL OR reps > 0),
    CHECK ((reps IS NOT NULL) + (duration_seconds IS NOT NULL)
           + (distance_mm IS NOT NULL) <= 1)
) STRICT, WITHOUT ROWID;
