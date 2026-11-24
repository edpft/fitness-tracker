-- no-transaction
PRAGMA foreign_keys = OFF;

CREATE TABLE canonical_gym_exercise_edit_rebuilt (
    edit     INTEGER NOT NULL REFERENCES canonical_gym_edit(id) ON DELETE CASCADE,
    session  INTEGER NOT NULL REFERENCES canonical_gym_session_identity(id),
    exercise INTEGER NOT NULL REFERENCES canonical_gym_exercise_identity(id),
    becomes  TEXT    NOT NULL,
    read     TEXT    NOT NULL,
    sets     TEXT    NOT NULL CHECK (json_valid(sets)),
    PRIMARY KEY (edit, exercise)
) STRICT, WITHOUT ROWID;

INSERT INTO canonical_gym_exercise_edit_rebuilt (edit, session, exercise, becomes, read, sets)
SELECT x.edit,
       coalesce(
           (SELECT e.session FROM canonical_gym_exercise AS e WHERE e.id = x.exercise),
           (SELECT p.session FROM canonical_gym_source_set AS p WHERE p.exercise = x.exercise
             LIMIT 1)),
       x.exercise, x.becomes, x.read, x.sets
FROM canonical_gym_exercise_edit AS x;

DROP TABLE canonical_gym_exercise_edit;
ALTER TABLE canonical_gym_exercise_edit_rebuilt RENAME TO canonical_gym_exercise_edit;

CREATE TABLE canonical_gym_set_edit_rebuilt (
    edit             INTEGER NOT NULL REFERENCES canonical_gym_edit(id) ON DELETE CASCADE,
    session          INTEGER NOT NULL REFERENCES canonical_gym_session_identity(id),
    exercise         INTEGER NOT NULL REFERENCES canonical_gym_exercise_identity(id),
    canonical_set    INTEGER NOT NULL REFERENCES canonical_gym_set_identity(id),
    reps             INTEGER CHECK (reps IS NULL OR reps > 0),
    duration_seconds INTEGER CHECK (duration_seconds IS NULL OR duration_seconds > 0),
    distance_mm      INTEGER CHECK (distance_mm IS NULL OR distance_mm > 0),
    load_kind        TEXT    CHECK (load_kind IN ('absolute', 'relative')),
    load_grams       INTEGER,
    rir              TEXT,
    PRIMARY KEY (edit, canonical_set),
    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK (load_kind IS NOT 'absolute' OR load_grams >= 0),
    CHECK ((reps IS NOT NULL) + (duration_seconds IS NOT NULL) + (distance_mm IS NOT NULL) <= 1),
    CHECK (reps IS NOT NULL OR duration_seconds IS NOT NULL OR distance_mm IS NOT NULL
           OR load_kind IS NOT NULL OR rir IS NOT NULL)
) STRICT, WITHOUT ROWID;

INSERT INTO canonical_gym_set_edit_rebuilt
    (edit, session, exercise, canonical_set, reps, duration_seconds, distance_mm,
     load_kind, load_grams, rir)
SELECT x.edit,
       coalesce(
           (SELECT st.session FROM canonical_gym_set AS st WHERE st.id = x.canonical_set),
           (SELECT p.session FROM canonical_gym_source_set AS p
             WHERE p.canonical_set = x.canonical_set LIMIT 1)),
       coalesce(
           (SELECT e.id FROM canonical_gym_set AS st
              JOIN canonical_gym_exercise AS e
                ON e.session = st.session AND e.item_position = st.item_position
               AND e.position = st.exercise_position
             WHERE st.id = x.canonical_set),
           (SELECT p.exercise FROM canonical_gym_source_set AS p
             WHERE p.canonical_set = x.canonical_set LIMIT 1)),
       x.canonical_set, x.reps, x.duration_seconds, x.distance_mm,
       x.load_kind, x.load_grams, x.rir
FROM canonical_gym_set_edit AS x;

DROP TABLE canonical_gym_set_edit;
ALTER TABLE canonical_gym_set_edit_rebuilt RENAME TO canonical_gym_set_edit;

CREATE TABLE canonical_gym_session_edit (
    edit           INTEGER NOT NULL REFERENCES canonical_gym_edit(id) ON DELETE CASCADE,
    session        INTEGER NOT NULL REFERENCES canonical_gym_session_identity(id),
    started_from   TEXT,
    started_at_utc TEXT,
    started_zone   TEXT,
    started_on_day TEXT,
    ended_from     TEXT,
    ended_at_utc   TEXT,
    PRIMARY KEY (edit, session),
    CHECK ((started_at_utc IS NULL) = (started_zone IS NULL)),
    CHECK (started_at_utc IS NULL OR started_on_day IS NULL),
    CHECK (started_from IS NULL OR (started_at_utc IS NULL AND started_on_day IS NULL)),
    CHECK (ended_from IS NULL OR ended_at_utc IS NULL),
    CHECK (started_from IS NOT NULL OR started_at_utc IS NOT NULL OR started_on_day IS NOT NULL
           OR ended_from IS NOT NULL OR ended_at_utc IS NOT NULL)
) STRICT, WITHOUT ROWID;

DELETE FROM canonical_gym_set;
DELETE FROM canonical_gym_exercise;
DELETE FROM canonical_gym_item;
DELETE FROM canonical_gym_session_heart_rate;
DELETE FROM canonical_gym_session;
DELETE FROM canonical_changed_day WHERE layer = 'gym';
DELETE FROM canonical_derivation WHERE layer = 'gym';

DROP TABLE canonical_gym_session;

CREATE TABLE canonical_gym_session (
    id                            INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at_utc                TEXT,
    zone                          TEXT,
    on_day                        TEXT,
    started_normalised_session    INTEGER REFERENCES gym_session(id),
    ended_at_utc                  TEXT,
    ended_normalised_session      INTEGER REFERENCES gym_session(id),
    average_bpm                   INTEGER,
    highest_bpm                   INTEGER,
    heart_rate_normalised_session INTEGER REFERENCES gym_session(id),
    CHECK ((on_day IS NULL) = (started_at_utc IS NOT NULL)),
    CHECK ((started_at_utc IS NULL) = (zone IS NULL)),
    CHECK (ended_at_utc IS NULL OR started_at_utc IS NOT NULL),
    CHECK (ended_at_utc IS NOT NULL OR ended_normalised_session IS NULL),
    CHECK ((average_bpm IS NULL) = (highest_bpm IS NULL)),
    CHECK ((average_bpm IS NULL) = (heart_rate_normalised_session IS NULL)),
    CHECK (average_bpm IS NULL OR average_bpm > 0),
    CHECK (highest_bpm IS NULL OR highest_bpm > 0)
) STRICT;

CREATE INDEX canonical_gym_session_by_day ON canonical_gym_session (on_day)
    WHERE on_day IS NOT NULL;
CREATE INDEX canonical_gym_session_by_instant ON canonical_gym_session (started_at_utc)
    WHERE started_at_utc IS NOT NULL;

CREATE TABLE canonical_gym_session_account (
    session            INTEGER NOT NULL REFERENCES canonical_gym_session(id),
    normalised_session INTEGER NOT NULL REFERENCES gym_session(id),
    stream             TEXT    NOT NULL,
    started_at_utc     TEXT,
    zone               TEXT,
    on_day             TEXT,
    ended_at_utc       TEXT,
    PRIMARY KEY (session, normalised_session),
    CHECK ((on_day IS NULL) = (started_at_utc IS NOT NULL)),
    CHECK ((started_at_utc IS NULL) = (zone IS NULL)),
    CHECK (ended_at_utc IS NULL OR started_at_utc IS NOT NULL)
) STRICT, WITHOUT ROWID;

PRAGMA foreign_keys = ON;
