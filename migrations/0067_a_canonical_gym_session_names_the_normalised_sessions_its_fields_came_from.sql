CREATE TABLE canonical_gym_session (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at_utc     TEXT,
    zone               TEXT,
    on_day             TEXT,
    duration_seconds   INTEGER,
    duration_normalised_session   INTEGER REFERENCES gym_session(id),
    average_bpm        INTEGER,
    highest_bpm        INTEGER,
    heart_rate_normalised_session INTEGER REFERENCES gym_session(id),
    CHECK ((on_day IS NULL) = (started_at_utc IS NOT NULL)),
    CHECK ((started_at_utc IS NULL) = (zone IS NULL)),
    CHECK ((duration_seconds IS NULL) = (duration_normalised_session IS NULL)),
    CHECK (duration_seconds IS NULL OR duration_seconds > 0),
    CHECK ((average_bpm IS NULL) = (highest_bpm IS NULL)),
    CHECK ((average_bpm IS NULL) = (heart_rate_normalised_session IS NULL)),
    CHECK (average_bpm IS NULL OR average_bpm > 0),
    CHECK (highest_bpm IS NULL OR highest_bpm > 0)
) STRICT;

CREATE TABLE canonical_gym_session_heart_rate (
    session          INTEGER NOT NULL REFERENCES canonical_gym_session(id),
    at_seconds       INTEGER NOT NULL,
    beats_per_minute INTEGER NOT NULL CHECK (beats_per_minute > 0),
    PRIMARY KEY (session, at_seconds),
    CHECK (at_seconds >= 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE canonical_gym_item (
    session     INTEGER NOT NULL REFERENCES canonical_gym_session(id),
    position    INTEGER NOT NULL,
    is_superset INTEGER NOT NULL CHECK (is_superset IN (0, 1)),
    PRIMARY KEY (session, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE canonical_gym_exercise (
    session            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    position           INTEGER NOT NULL,
    measure            TEXT    NOT NULL CHECK (measure IN ('reps', 'duration', 'distance')),
    identified         TEXT    NOT NULL
        CHECK (identified IN ('recorded', 'proposed', 'from-its-run', 'undetermined')),
    exercise           TEXT,
    movement           TEXT,
    implement          TEXT,
    identified_normalised_session INTEGER NOT NULL REFERENCES gym_session(id),
    PRIMARY KEY (session, item_position, position),
    FOREIGN KEY (session, item_position) REFERENCES canonical_gym_item(session, position),
    CHECK (exercise IS NULL OR movement IS NULL),
    CHECK (implement IS NULL OR movement IS NOT NULL),
    CHECK ((identified = 'undetermined') = (exercise IS NULL AND movement IS NULL)),
    CHECK (measure = 'reps' OR identified = 'recorded')
) STRICT, WITHOUT ROWID;

CREATE TABLE canonical_gym_set (
    session            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    exercise_position  INTEGER NOT NULL,
    position           INTEGER NOT NULL,
    outcome            TEXT    NOT NULL CHECK (outcome IN ('completed', 'failed')),
    reps               INTEGER,
    duration_seconds   INTEGER,
    distance_mm        INTEGER,
    outcome_normalised_session    INTEGER NOT NULL REFERENCES gym_session(id),
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
    CHECK ((load_kind IS NULL) = (load_normalised_session IS NULL)),
    CHECK (load_kind IS NOT 'absolute' OR load_grams >= 0),
    CHECK ((began_at_utc IS NULL) = (began_zone IS NULL)),
    CHECK ((began_at_utc IS NULL) = (began_normalised_session IS NULL)),
    CHECK ((rir IS NULL) = (rir_normalised_session IS NULL)),
    CHECK ((set_kind IS NULL) = (set_kind_normalised_session IS NULL)),
    CHECK ((rest_after_seconds IS NULL) = (rest_after_normalised_session IS NULL)),
    CHECK (outcome != 'failed'
           OR (reps IS NULL AND duration_seconds IS NULL AND distance_mm IS NULL)),
    CHECK (reps IS NULL OR reps > 0),
    CHECK ((reps IS NOT NULL) + (duration_seconds IS NOT NULL)
           + (distance_mm IS NOT NULL) <= 1)
) STRICT, WITHOUT ROWID;

CREATE INDEX canonical_gym_session_by_day ON canonical_gym_session (on_day)
    WHERE on_day IS NOT NULL;

CREATE INDEX canonical_gym_session_by_instant ON canonical_gym_session (started_at_utc)
    WHERE started_at_utc IS NOT NULL;
