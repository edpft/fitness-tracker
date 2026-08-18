-- The two templates are two models of periodisation, not two versions of one
-- programme, so `v1` becomes `linear` and `v2` becomes `block`.
--
-- Settled with the operator on 2026-08-18. `v1` and `v2` said "the second
-- replaces the first", which is the opposite of what is true: a linear top-set
-- ladder is the right tool for a short or interrupted window and block
-- periodisation is the right tool for a long one, and a programme picks between
-- them on how many weeks the calendar gives. Nothing about the loading changes
-- here — only the name, and the CHECK that admits it.
--
-- `'block'` is admitted now although nothing can write one yet: the column exists
-- to name which template a programme was generated against, and listing both is
-- what makes adding the second one a store write rather than a migration.
--
-- **Relaxing one CHECK costs nine tables**, which is worth knowing before
-- reaching for another. A CHECK cannot be altered in place, so `programme` is
-- rebuilt; renaming it re-points the foreign keys of every table that references
-- it, and `ALTER TABLE ... RENAME` does that whether or not `legacy_alter_table`
-- is on — verified, and it is why that pragma does not appear below. So every
-- table whose reference the rename rewrites is rebuilt too, and the chain from
-- `prescribed_workout` down to `prescribed_set` comes with it.
--
-- The technique and its reasoning are `0006`'s: drop the globally-named indexes,
-- rename aside so old references old, create and fill parents-first, drop the old
-- copies children-first. Nothing is deleted while anything points at it, so
-- foreign keys stay on and this is one transaction. The definitions are `0006`'s
-- verbatim apart from `programme.template`, and `programme_slot_fill` is still
-- the one table STRICT cannot take — see `0006` for why.

DROP INDEX programme_current;

ALTER TABLE programme RENAME TO programme_old;
ALTER TABLE programme_slot_fill RENAME TO programme_slot_fill_old;
ALTER TABLE programme_weekday RENAME TO programme_weekday_old;
ALTER TABLE programme_interruption RENAME TO programme_interruption_old;
ALTER TABLE prescribed_workout RENAME TO prescribed_workout_old;
ALTER TABLE prescribed_item RENAME TO prescribed_item_old;
ALTER TABLE prescribed_slot RENAME TO prescribed_slot_old;
ALTER TABLE prescribed_exercise RENAME TO prescribed_exercise_old;
ALTER TABLE prescribed_set RENAME TO prescribed_set_old;

CREATE TABLE programme (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
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

    gating_role          TEXT    NOT NULL CHECK (gating_role IN ('light', 'heavy')),
    start_date           TEXT    NOT NULL,

    duration_weeks       INTEGER NOT NULL CHECK (duration_weeks >= 2)
) STRICT;

INSERT INTO programme (
    id, authored_at, template, primary_pattern, primary_exercise,
    anchor_grams, anchor_provenance, anchor_from, gating_role, start_date,
    duration_weeks
)
SELECT
    id, authored_at,
    CASE template WHEN 'v1' THEN 'linear' ELSE template END,
    primary_pattern, primary_exercise, anchor_grams, anchor_provenance,
    anchor_from, gating_role, start_date, duration_weeks
FROM programme_old;

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

INSERT INTO programme_slot_fill (
    programme, slot, role, position, exercise, static_sets, static_reps
)
SELECT
    programme, slot, role, position, exercise, static_sets, static_reps
FROM programme_slot_fill_old;

CREATE TABLE programme_weekday (
    programme  INTEGER NOT NULL REFERENCES programme(id),
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    role       TEXT    NOT NULL CHECK (role IN ('light', 'heavy')),

    PRIMARY KEY (programme, weekday)
) STRICT, WITHOUT ROWID;

INSERT INTO programme_weekday (
    programme, weekday, role
)
SELECT
    programme, weekday, role
FROM programme_weekday_old;

CREATE TABLE programme_interruption (
    programme INTEGER NOT NULL REFERENCES programme(id),

    week      TEXT    NOT NULL,

    PRIMARY KEY (programme, week)
) STRICT, WITHOUT ROWID;

INSERT INTO programme_interruption (
    programme, week
)
SELECT
    programme, week
FROM programme_interruption_old;

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

    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    issued_at              TEXT    NOT NULL,

    UNIQUE (issued_for),

    CHECK ((week_kind = 'climbing') = (week_index IS NOT NULL))
) STRICT;

INSERT INTO prescribed_workout (
    id, programme, issued_for, zone, session_role, week_kind, week_index,
    anchor_grams, anchor_provenance, anchor_from, parameters_authored_at,
    issued_at
)
SELECT
    id, programme, issued_for, zone, session_role, week_kind, week_index,
    anchor_grams, anchor_provenance, anchor_from, parameters_authored_at,
    issued_at
FROM prescribed_workout_old;

CREATE TABLE prescribed_item (
    workout      INTEGER NOT NULL REFERENCES prescribed_workout(id),
    position     INTEGER NOT NULL,
    is_superset  INTEGER NOT NULL CHECK (is_superset IN (0, 1)),

    PRIMARY KEY (workout, position)
) STRICT, WITHOUT ROWID;

INSERT INTO prescribed_item (
    workout, position, is_superset
)
SELECT
    workout, position, is_superset
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

INSERT INTO prescribed_slot (
    workout, item_position, member_position, slot
)
SELECT
    workout, item_position, member_position, slot
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

INSERT INTO prescribed_exercise (
    workout, item_position, position, exercise, measure
)
SELECT
    workout, item_position, position, exercise, measure
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

INSERT INTO prescribed_set (
    workout, item_position, exercise_position, position, variant, load_kind,
    load_grams, target_kind, target_low, target_high, effort,
    rest_low_seconds, rest_high_seconds, warmup
)
SELECT
    workout, item_position, exercise_position, position, variant, load_kind,
    load_grams, target_kind, target_low, target_high, effort,
    rest_low_seconds, rest_high_seconds, warmup
FROM prescribed_set_old;

DROP TABLE prescribed_set_old;
DROP TABLE prescribed_exercise_old;
DROP TABLE prescribed_slot_old;
DROP TABLE prescribed_item_old;
DROP TABLE prescribed_workout_old;
DROP TABLE programme_interruption_old;
DROP TABLE programme_weekday_old;
DROP TABLE programme_slot_fill_old;
DROP TABLE programme_old;

CREATE INDEX programme_current ON programme (authored_at DESC);
