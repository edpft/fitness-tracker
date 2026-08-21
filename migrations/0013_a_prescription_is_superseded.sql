-- A prescription is superseded, not overwritten.
--
-- `UNIQUE (issued_for)` allowed one prescription per date for all time. That is
-- right about the *current* answer and wrong about the record: re-authoring the
-- programme on 2026-08-20 left 2026-08-21 already answered by a prescription
-- derived from the superseded one, and nothing could replace it.
--
-- So the key gains `issued_at` and reads take the greatest. This is the rule
-- `generation_parameters` has always used — "the one in force is the greatest
-- `authored_at`, which is a WHERE clause rather than a mutable flag" — applied
-- to the other half of § 12's authored data. Nothing is deleted and nothing is
-- mutated: the superseded prescription stays exactly as it was issued, which is
-- what makes expectation against reality recoverable (§ 11).
--
-- Five tables rather than twelve: only `prescribed_workout` changes, and only
-- its own children reference it. `0006`'s order and reasons otherwise.

-- 2. Rename aside. Old now references old.

ALTER TABLE prescribed_workout   RENAME TO prescribed_workout_old;
ALTER TABLE prescribed_item      RENAME TO prescribed_item_old;
ALTER TABLE prescribed_slot      RENAME TO prescribed_slot_old;
ALTER TABLE prescribed_exercise  RENAME TO prescribed_exercise_old;
ALTER TABLE prescribed_set       RENAME TO prescribed_set_old;

-- 3. Create and fill, parents-first.

CREATE TABLE prescribed_workout (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    programme              INTEGER NOT NULL REFERENCES programme(id),

    issued_for             TEXT    NOT NULL,
    zone                   TEXT    NOT NULL,
    session_role           TEXT    NOT NULL CHECK (session_role IN ('light', 'heavy')),

    week_kind              TEXT    NOT NULL CHECK (week_kind IN ('climbing', 'test')),
    week_index             INTEGER,

    anchor_grams           INTEGER NOT NULL CHECK (anchor_grams > 0),
    anchor_provenance      TEXT    NOT NULL
        CHECK (anchor_provenance IN ('tested', 'estimated', 'asserted')),
    anchor_from            TEXT    NOT NULL,
    -- What the entry test failed above `anchor_grams`, if it found the
    -- ceiling. Null is a test that did not.
    anchor_failed_grams  INTEGER
        CHECK (anchor_failed_grams > anchor_grams),

    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    issued_at              TEXT    NOT NULL,

    -- A date may be prescribed more than once, and the latest issue is the
    -- one in force. Same rule as `generation_parameters`, for the same reason:
    -- an issued prescription is authored data (§ 12) and keeps its history, so
    -- correcting one supersedes it rather than overwriting it. `UNIQUE
    -- (issued_for)` said a date could be answered once ever, which made a
    -- programme authored after a prescription unable to correct it.
    UNIQUE (issued_for, issued_at),

    CHECK ((week_kind = 'climbing') = (week_index IS NOT NULL))
) STRICT;

INSERT INTO prescribed_workout (id, programme, issued_for, zone, session_role, week_kind, week_index, anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams, parameters_authored_at, issued_at)
SELECT id, programme, issued_for, zone, session_role, week_kind, week_index, anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams, parameters_authored_at, issued_at
FROM prescribed_workout_old;

CREATE TABLE prescribed_item (
    workout      INTEGER NOT NULL REFERENCES prescribed_workout(id),
    position     INTEGER NOT NULL,
    is_superset  INTEGER NOT NULL CHECK (is_superset IN (0, 1)),

    PRIMARY KEY (workout, position)
) STRICT, WITHOUT ROWID;

INSERT INTO prescribed_item (workout, position, is_superset)
SELECT workout, position, is_superset
FROM prescribed_item_old;

CREATE TABLE prescribed_slot (
    workout          INTEGER NOT NULL,
    item_position    INTEGER NOT NULL,
    member_position  INTEGER NOT NULL,
    slot             TEXT    NOT NULL,

    PRIMARY KEY (workout, item_position, member_position),
    FOREIGN KEY (workout, item_position)
        REFERENCES prescribed_item(workout, position)
) STRICT, WITHOUT ROWID;

INSERT INTO prescribed_slot (workout, item_position, member_position, slot)
SELECT workout, item_position, member_position, slot
FROM prescribed_slot_old;

CREATE TABLE prescribed_exercise (
    workout        INTEGER NOT NULL,
    item_position  INTEGER NOT NULL,
    position       INTEGER NOT NULL,
    exercise       TEXT    NOT NULL,
    measure        TEXT    NOT NULL CHECK (measure IN ('reps', 'duration', 'distance')),

    PRIMARY KEY (workout, item_position, position),
    FOREIGN KEY (workout, item_position)
        REFERENCES prescribed_item(workout, position)
) STRICT, WITHOUT ROWID;

INSERT INTO prescribed_exercise (workout, item_position, position, exercise, measure)
SELECT workout, item_position, position, exercise, measure
FROM prescribed_exercise_old;

CREATE TABLE prescribed_set (
    workout            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    exercise_position  INTEGER NOT NULL,
    position           INTEGER NOT NULL,

    variant            TEXT    NOT NULL
        CHECK (variant IN ('fixed', 'to_effort', 'autoregulated')),

    load_kind          TEXT    CHECK (load_kind IN ('absolute', 'relative')),
    load_grams         INTEGER,

    target_kind        TEXT    CHECK (target_kind IN ('reps', 'duration', 'distance')),
    target_low         INTEGER,
    target_high        INTEGER,

    effort             TEXT,

    rest_low_seconds   INTEGER,
    rest_high_seconds  INTEGER,

    warmup             INTEGER NOT NULL CHECK (warmup IN (0, 1)),

    PRIMARY KEY (workout, item_position, exercise_position, position),
    FOREIGN KEY (workout, item_position, exercise_position)
        REFERENCES prescribed_exercise(workout, item_position, position),

    CHECK (variant != 'fixed'
           OR (load_kind IS NOT NULL AND target_kind IS NOT NULL)),

    CHECK (variant != 'to_effort'
           OR (load_kind IS NOT NULL AND effort IS NOT NULL)),

    CHECK (variant != 'autoregulated'
           OR (load_kind IS NULL AND target_kind IS NOT NULL AND effort IS NOT NULL)),

    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK ((target_kind IS NULL) = (target_low IS NULL)),
    CHECK (target_high IS NULL OR target_low IS NOT NULL),

    CHECK (target_high IS NULL OR target_high > target_low),
    CHECK ((rest_high_seconds IS NULL) OR (rest_low_seconds IS NOT NULL)),
    CHECK (rest_high_seconds IS NULL OR rest_high_seconds > rest_low_seconds)
) STRICT, WITHOUT ROWID;

INSERT INTO prescribed_set (workout, item_position, exercise_position, position, variant, load_kind, load_grams, target_kind, target_low, target_high, effort, rest_low_seconds, rest_high_seconds, warmup)
SELECT workout, item_position, exercise_position, position, variant, load_kind, load_grams, target_kind, target_low, target_high, effort, rest_low_seconds, rest_high_seconds, warmup
FROM prescribed_set_old;

-- 4. Drop the old copies, children-first.

DROP TABLE prescribed_set_old;
DROP TABLE prescribed_exercise_old;
DROP TABLE prescribed_slot_old;
DROP TABLE prescribed_item_old;
DROP TABLE prescribed_workout_old;
