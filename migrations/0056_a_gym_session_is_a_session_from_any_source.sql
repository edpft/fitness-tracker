PRAGMA defer_foreign_keys = ON;

DROP TRIGGER prescription_delivery_performed_is_not_deletable;
DROP TRIGGER prescribed_workout_performed_is_not_deletable;

CREATE TABLE gym_session_next (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    stream             TEXT    NOT NULL,
    landing_record_id  INTEGER NOT NULL,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

INSERT INTO gym_session_next (id, stream, landing_record_id, run_id)
SELECT landing_record_id, 'hevy.workouts', landing_record_id, run_id
FROM gym_session;

CREATE TABLE gym_workout_next (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    session            INTEGER NOT NULL REFERENCES gym_session_next(id),
    stream             TEXT    NOT NULL,
    landing_record_id  INTEGER NOT NULL,
    source_record_id   TEXT    NOT NULL,
    started_at_utc     TEXT,
    zone               TEXT,
    on_day             TEXT,
    endpoint           TEXT,
    event_kind         TEXT,
    event_time         TEXT,
    performed_against  TEXT,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id),
    CHECK ((on_day IS NULL) = (started_at_utc IS NOT NULL)),
    CHECK ((started_at_utc IS NULL) = (zone IS NULL)),
    CHECK ((endpoint IS NULL) = (event_kind IS NULL)),
    CHECK (event_time IS NULL OR endpoint IS NOT NULL)
) STRICT;

INSERT INTO gym_workout_next (
    id, session, stream, landing_record_id, source_record_id, started_at_utc, zone,
    endpoint, event_kind, event_time, performed_against, run_id
)
SELECT landing_record_id, session, 'hevy.workouts', landing_record_id, source_record_id,
       started_at_utc, zone, endpoint, event_kind, event_time, performed_against, run_id
FROM gym_workout
WHERE session IS NOT NULL;

CREATE TABLE workout_item_next (
    workout       INTEGER NOT NULL REFERENCES gym_workout_next(id),
    position      INTEGER NOT NULL,
    is_superset   INTEGER NOT NULL CHECK (is_superset IN (0, 1)),
    PRIMARY KEY (workout, position)
) STRICT, WITHOUT ROWID;

INSERT INTO workout_item_next (workout, position, is_superset)
SELECT i.workout, i.position, i.is_superset
FROM workout_item AS i
JOIN gym_workout_next AS w ON w.id = i.workout;

CREATE TABLE performed_exercise_next (
    workout        INTEGER NOT NULL,
    item_position  INTEGER NOT NULL,
    position       INTEGER NOT NULL,
    exercise       TEXT    NOT NULL,
    measure        TEXT    NOT NULL CHECK (measure IN ('reps', 'duration', 'distance')),
    PRIMARY KEY (workout, item_position, position),
    FOREIGN KEY (workout, item_position) REFERENCES workout_item_next(workout, position)
) STRICT, WITHOUT ROWID;

INSERT INTO performed_exercise_next (workout, item_position, position, exercise, measure)
SELECT e.workout, e.item_position, e.position, e.exercise, e.measure
FROM performed_exercise AS e
JOIN gym_workout_next AS w ON w.id = e.workout;

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
    PRIMARY KEY (workout, item_position, exercise_position, position),
    FOREIGN KEY (workout, item_position, exercise_position)
        REFERENCES performed_exercise_next(workout, item_position, position),
    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK ((sheet IS NULL) = (cell IS NULL)),
    CHECK (load_kind IS NOT 'absolute' OR load_grams >= 0),
    CHECK (outcome != 'failed'
           OR (reps IS NULL AND duration_seconds IS NULL AND distance_mm IS NULL)),
    CHECK (outcome != 'completed'
           OR (reps IS NOT NULL) + (duration_seconds IS NOT NULL)
              + (distance_mm IS NOT NULL) >= 1),
    CHECK (reps IS NULL OR (duration_seconds IS NULL AND distance_mm IS NULL)),
    CHECK (reps IS NULL OR reps > 0)
) STRICT, WITHOUT ROWID;

INSERT INTO performed_set_next (
    workout, item_position, exercise_position, position, load_kind, load_grams, outcome,
    reps, duration_seconds, distance_mm, rir, set_kind, rest_after_seconds
)
SELECT s.workout, s.item_position, s.exercise_position, s.position, s.load_kind,
       s.load_grams, s.outcome, s.reps, s.duration_seconds, s.distance_mm, s.rir,
       s.set_kind, s.rest_after_seconds
FROM performed_set AS s
JOIN gym_workout_next AS w ON w.id = s.workout;

DROP TABLE performed_set;
DROP TABLE performed_exercise;
DROP TABLE workout_item;
DROP TABLE gym_workout;
DROP TABLE gym_session;

ALTER TABLE gym_session_next RENAME TO gym_session;
ALTER TABLE gym_workout_next RENAME TO gym_workout;
ALTER TABLE workout_item_next RENAME TO workout_item;
ALTER TABLE performed_exercise_next RENAME TO performed_exercise;
ALTER TABLE performed_set_next RENAME TO performed_set;

CREATE UNIQUE INDEX gym_session_hevy_by_landing_record
    ON gym_session (landing_record_id)
    WHERE stream = 'hevy.workouts';

CREATE INDEX gym_session_by_stream ON gym_session (stream);

CREATE UNIQUE INDEX gym_workout_hevy_by_landing_record
    ON gym_workout (landing_record_id)
    WHERE stream = 'hevy.workouts';

CREATE INDEX gym_workout_by_stream ON gym_workout (stream);

CREATE INDEX gym_workout_by_source_record ON gym_workout (source_record_id);

CREATE INDEX gym_workout_by_session ON gym_workout (performed_against)
    WHERE performed_against IS NOT NULL;

CREATE INDEX gym_workout_by_session_composed ON gym_workout (session);

CREATE TRIGGER prescription_delivery_performed_is_not_deletable
BEFORE DELETE ON prescription_delivery
WHEN EXISTS (
    SELECT 1 FROM gym_workout WHERE performed_against = OLD.reference
)
BEGIN
    SELECT RAISE(ABORT, 'a performed session is not withdrawable (constitution 12)');
END;

CREATE TRIGGER prescribed_workout_performed_is_not_deletable
BEFORE DELETE ON prescribed_workout
WHEN EXISTS (
    SELECT 1
    FROM prescription_delivery AS d
    JOIN gym_workout AS w ON w.performed_against = d.reference
    WHERE d.prescription = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'a performed prescription is not deletable (constitution 12)');
END;
