-- A programme is identified by a declared name, not by being the latest.
--
-- `programme` was read with `ORDER BY authored_at DESC LIMIT 1`: one programme
-- was in force, and authoring another replaced it. That collapses two different
-- relations into one (decision 0012):
--
--   supersession   the same programme, re-authored to correct it
--   succession     a different programme, later in time
--
-- Under the first, latest wins. Under the second both are real, and the date
-- being asked about decides which one answers. A periodised block following a
-- linear one is succession, and it was not expressible.
--
-- **The name is declared, never inferred.** The start date was the obvious
-- natural key and is wrong: correcting a start date would silently fork a new
-- programme rather than amend the existing one. The document carries the name,
-- so the authored record stays reproducible from the document alone (§ 12).
--
-- **Existing rows collapse into one programme rather than being deleted.** All
-- five are development iterations of one front squat block -- four share a
-- start date outright, and the fifth is its 003-era ancestor. Giving them one
-- name makes them five versions of one programme, so the latest is read and the
-- rest are history. Nothing is dropped, and every `prescribed_workout` row goes
-- on pointing at the exact version it was issued from.
--
-- The name they collapse to is the one the operator's document must carry when
-- the summer block is re-authored under decision 0013. A differently named
-- programme covering the same dates would be refused as an overlap, which is
-- the rule working rather than a fault.
--
-- **Nine tables, in 0006's order, for one column.** SQLite rewrites a child's
-- `REFERENCES` clause when its parent is renamed, so renaming `programme`
-- alone would leave four tables pointing at `programme_old` -- and renaming
-- those to fix them rewrites *their* children in turn. `PRAGMA
-- legacy_alter_table` would stop the rewrite and is a no-op inside the
-- transaction each migration runs in, so the whole graph is rebuilt instead.
-- This is 0012's procedure, for the same reason.

ALTER TABLE programme              RENAME TO programme_old;
ALTER TABLE programme_interruption RENAME TO programme_interruption_old;
ALTER TABLE programme_slot_fill    RENAME TO programme_slot_fill_old;
ALTER TABLE programme_weekday      RENAME TO programme_weekday_old;
ALTER TABLE prescribed_workout     RENAME TO prescribed_workout_old;
ALTER TABLE prescribed_item        RENAME TO prescribed_item_old;
ALTER TABLE prescribed_slot        RENAME TO prescribed_slot_old;
ALTER TABLE prescribed_exercise    RENAME TO prescribed_exercise_old;
ALTER TABLE prescribed_set         RENAME TO prescribed_set_old;

CREATE TABLE programme (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    -- What identifies this programme across re-authorings (decision 0012).
    -- Free text: it is the operator's own label, and the only rules are the
    -- ones a label needs to be an identity -- non-empty, one printable line,
    -- trimmed. Two rows sharing it are one programme's versions.
    name                 TEXT    NOT NULL,

    authored_at          TEXT    NOT NULL,
    template             TEXT    NOT NULL CHECK (template IN ('linear', 'block')),

    primary_pattern      TEXT    NOT NULL
        CHECK (primary_pattern IN ('knee_dominant', 'hip_dominant',
                                   'upper_push', 'upper_pull')),
    primary_exercise     TEXT    NOT NULL,

    anchor_grams         INTEGER NOT NULL CHECK (anchor_grams > 0),
    anchor_provenance    TEXT    NOT NULL
        CHECK (anchor_provenance IN ('tested', 'estimated', 'asserted')),
    anchor_from          TEXT    NOT NULL,
    -- Where the ladder opens, where the block states it rather than deriving it
    -- from the anchor above. Null derives it. Not a default: a block picked up
    -- mid-flight, or one starting long enough after its test that nothing off
    -- that test is evidence any more, has an opening no derivation can reach.
    opening_grams        INTEGER CHECK (opening_grams > 0),
    -- What the entry test failed above `anchor_grams`, if it found the
    -- ceiling. Null is a test that did not.
    anchor_failed_grams  INTEGER
        CHECK (anchor_failed_grams > anchor_grams),

    gating_role          TEXT    NOT NULL CHECK (gating_role IN ('light', 'heavy')),
    start_date           TEXT    NOT NULL,

    duration_weeks       INTEGER NOT NULL CHECK (duration_weeks >= 2),

    -- One authoring of one programme. Two rows claiming to be the same version
    -- would make which of them is read depend on the surrogate key.
    UNIQUE (name, authored_at)
) STRICT;

CREATE TABLE programme_interruption (
    programme  INTEGER NOT NULL REFERENCES programme(id),

    -- The first day the block does not run.
    start_date TEXT    NOT NULL,
    -- How many consecutive days it covers, itself included. At least one: a
    -- skip of no days would author successfully and skip nothing.
    days       INTEGER NOT NULL CHECK (days >= 1 AND days <= 255),

    PRIMARY KEY (programme, start_date)
) STRICT, WITHOUT ROWID;

CREATE TABLE programme_slot_fill (
    programme  INTEGER NOT NULL REFERENCES programme(id),
    slot       TEXT    NOT NULL,
    role       TEXT    CHECK (role IS NULL OR role IN ('light', 'heavy')),

    position   INTEGER NOT NULL DEFAULT 0,
    exercise   TEXT    NOT NULL,

    static_sets INTEGER CHECK (static_sets IS NULL OR static_sets > 0),
    static_reps INTEGER CHECK (static_reps IS NULL OR static_reps > 0),
    CHECK ((static_sets IS NULL) = (static_reps IS NULL)),

    PRIMARY KEY (programme, slot, role, position)
);

CREATE TABLE programme_weekday (
    programme  INTEGER NOT NULL REFERENCES programme(id),
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    role       TEXT    NOT NULL CHECK (role IN ('light', 'heavy')),

    PRIMARY KEY (programme, weekday)
) STRICT, WITHOUT ROWID;

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

CREATE TABLE prescribed_item (
    workout      INTEGER NOT NULL REFERENCES prescribed_workout(id),
    position     INTEGER NOT NULL,
    is_superset  INTEGER NOT NULL CHECK (is_superset IN (0, 1)),

    PRIMARY KEY (workout, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE prescribed_slot (
    workout          INTEGER NOT NULL,
    item_position    INTEGER NOT NULL,
    member_position  INTEGER NOT NULL,
    slot             TEXT    NOT NULL,

    PRIMARY KEY (workout, item_position, member_position),
    FOREIGN KEY (workout, item_position)
        REFERENCES prescribed_item(workout, position)
) STRICT, WITHOUT ROWID;

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

INSERT INTO programme (id, name, authored_at, template, primary_pattern, primary_exercise, anchor_grams, anchor_provenance, anchor_from, opening_grams, anchor_failed_grams, gating_role, start_date, duration_weeks)
SELECT id, 'summer-2026-front-squat', authored_at, template, primary_pattern, primary_exercise, anchor_grams, anchor_provenance, anchor_from, opening_grams, anchor_failed_grams, gating_role, start_date, duration_weeks
FROM programme_old;

INSERT INTO programme_interruption (programme, start_date, days)
SELECT programme, start_date, days FROM programme_interruption_old;

INSERT INTO programme_slot_fill (programme, slot, role, position, exercise, static_sets, static_reps)
SELECT programme, slot, role, position, exercise, static_sets, static_reps FROM programme_slot_fill_old;

INSERT INTO programme_weekday (programme, weekday, role)
SELECT programme, weekday, role FROM programme_weekday_old;

INSERT INTO prescribed_workout (id, programme, issued_for, zone, session_role, week_kind, week_index, anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams, parameters_authored_at, issued_at)
SELECT id, programme, issued_for, zone, session_role, week_kind, week_index, anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams, parameters_authored_at, issued_at FROM prescribed_workout_old;

INSERT INTO prescribed_item (workout, position, is_superset)
SELECT workout, position, is_superset FROM prescribed_item_old;

INSERT INTO prescribed_slot (workout, item_position, member_position, slot)
SELECT workout, item_position, member_position, slot FROM prescribed_slot_old;

INSERT INTO prescribed_exercise (workout, item_position, position, exercise, measure)
SELECT workout, item_position, position, exercise, measure FROM prescribed_exercise_old;

INSERT INTO prescribed_set (workout, item_position, exercise_position, position, variant, load_kind, load_grams, target_kind, target_low, target_high, effort, rest_low_seconds, rest_high_seconds, warmup)
SELECT workout, item_position, exercise_position, position, variant, load_kind, load_grams, target_kind, target_low, target_high, effort, rest_low_seconds, rest_high_seconds, warmup FROM prescribed_set_old;

DROP TABLE prescribed_set_old;
DROP TABLE prescribed_exercise_old;
DROP TABLE prescribed_slot_old;
DROP TABLE prescribed_item_old;
DROP TABLE prescribed_workout_old;
DROP TABLE programme_weekday_old;
DROP TABLE programme_slot_fill_old;
DROP TABLE programme_interruption_old;
DROP TABLE programme_old;
