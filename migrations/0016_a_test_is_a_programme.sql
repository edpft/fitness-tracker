-- A test is a programme, and a programme need not have an anchor.
--
-- Decision 0013 makes a test a programme in its own right: one week, no ladder,
-- and no starting maximum, because producing one is the whole of what it does.
-- Every column this table made mandatory was mandatory for a programme that
-- climbs -- an anchor, the session that gates the climb, a duration of at least
-- two weeks -- and none of them is answerable by a test.
--
-- **Conditional rather than nullable-and-hope.** The obvious change is to drop
-- the `NOT NULL`s and leave it there, which would let a linear programme be
-- stored with no anchor and fail at rehydration instead of at insertion. Each
-- one becomes a `CHECK` keyed on the template, so what a row must carry is
-- decided by what kind of programme it is and the store cannot write a
-- half-formed one.
--
-- **Two columns arrive, both a test's own.** `test_reps` is what the attempt is
-- performed at -- a single before a linear programme, a triple before a block,
-- which is `block.rs`'s own long-standing reasoning now that the entry test has
-- moved out of the block. `test_target_grams` is what the attempt is *at*, and
-- is null for the ordinary case: decision 0011 makes the target a function of
-- where the predecessor's progression stands, so it moves as the record does and
-- a stored number would be stale the first time a session goes up. It is
-- recorded only where there is nothing to inherit from.
--
-- **A block's anchor must have been tested.** Decision 0013 makes provenance
-- load-bearing there and nowhere else: if an asserted anchor satisfied a block's
-- entry requirement, switching lifts could skip the standalone test by stating a
-- number. The domain refuses it and so does this, because a rule that only one
-- of the two enforces is a rule that drifts.
--
-- **`prescribed_workout` follows.** It records the anchor by value, and a
-- session issued from a test programme has none. It gains the target by value
-- for the same reason the anchor is there: the number moves, and what was issued
-- has to stay readable exactly as issued (§ 12).
--
-- **Nine tables again, in 0006's order, and for the same reason as 0015.**
-- SQLite rewrites a child's `REFERENCES` clause when its parent is renamed, so
-- rebuilding `programme` and `prescribed_workout` alone would leave seven tables
-- pointing at `*_old`. The whole graph is rebuilt instead.

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
    name                 TEXT    NOT NULL,

    authored_at          TEXT    NOT NULL,
    -- 'v1' until 2026-08-18, when linear and block stopped being versions of one
    -- thing; 'test' from 2026-08-22, when a test stopped being a week of one.
    template             TEXT    NOT NULL
        CHECK (template IN ('linear', 'block', 'test')),

    -- The lift this programme is about. For a test that is the lift being
    -- tested, which is the *next* programme's primary rather than the
    -- predecessor's -- so it is stated by every template.
    primary_pattern      TEXT    NOT NULL
        CHECK (primary_pattern IN ('knee_dominant', 'hip_dominant',
                                   'upper_push', 'upper_pull')),
    primary_exercise     TEXT    NOT NULL,

    -- The starting 1RM, for a programme that has one to start from.
    anchor_grams         INTEGER CHECK (anchor_grams IS NULL OR anchor_grams > 0),
    anchor_provenance    TEXT
        CHECK (anchor_provenance IS NULL
               OR anchor_provenance IN ('tested', 'estimated', 'asserted')),
    anchor_from          TEXT,
    -- Where the ladder opens, where the programme states it rather than deriving
    -- it from the anchor above. Null derives it.
    opening_grams        INTEGER CHECK (opening_grams IS NULL OR opening_grams > 0),
    -- What the entry test failed above `anchor_grams`, if it found the ceiling.
    anchor_failed_grams  INTEGER
        CHECK (anchor_failed_grams IS NULL OR anchor_failed_grams > anchor_grams),

    -- Which session's top set advances the plan. A test advances nothing: its
    -- own session is fixed at the heavy one and there is no ladder to gate.
    gating_role          TEXT    CHECK (gating_role IS NULL
                                        OR gating_role IN ('light', 'heavy')),
    start_date           TEXT    NOT NULL,
    duration_weeks       INTEGER NOT NULL,

    -- What a test is performed at, and what it is an attempt at.
    test_reps            INTEGER CHECK (test_reps IS NULL OR test_reps > 0),
    test_target_grams    INTEGER CHECK (test_target_grams IS NULL
                                        OR test_target_grams > 0),

    -- A test has no anchor, no opening and no gate; every other template has an
    -- anchor and a gate, and may have an opening.
    CHECK ((template = 'test') = (anchor_grams IS NULL)),
    CHECK ((anchor_grams IS NULL) = (anchor_provenance IS NULL)),
    CHECK ((anchor_grams IS NULL) = (anchor_from IS NULL)),
    CHECK ((template = 'test') = (gating_role IS NULL)),
    CHECK (template != 'test' OR opening_grams IS NULL),

    -- And only a test has a repetition count or a target.
    CHECK ((template = 'test') = (test_reps IS NOT NULL)),
    CHECK (template = 'test' OR test_target_grams IS NULL),

    -- A test is one week. Anything that climbs needs at least two, because a
    -- ladder of one rung is a load rather than a plan.
    CHECK (CASE template WHEN 'test' THEN duration_weeks = 1
                         ELSE duration_weeks >= 2 END),

    -- A block opens from a measured maximum, and only from one (decision 0013).
    CHECK (template != 'block' OR anchor_provenance = 'tested'),

    -- One authoring of one programme.
    UNIQUE (name, authored_at)
) STRICT;

CREATE TABLE programme_interruption (
    programme  INTEGER NOT NULL REFERENCES programme(id),

    start_date TEXT    NOT NULL,
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

    -- By value, so what was issued stays readable as issued. Null for a session
    -- issued from a test programme, which has no anchor to record.
    anchor_grams           INTEGER CHECK (anchor_grams IS NULL OR anchor_grams > 0),
    anchor_provenance      TEXT
        CHECK (anchor_provenance IS NULL
               OR anchor_provenance IN ('tested', 'estimated', 'asserted')),
    anchor_from            TEXT,
    anchor_failed_grams    INTEGER
        CHECK (anchor_failed_grams IS NULL OR anchor_failed_grams > anchor_grams),

    -- What a test session was an attempt at (decision 0011). By value for the
    -- same reason as the anchor, and more so: it is a function of where the
    -- record stood when the session was issued, so nothing can recompute what
    -- it was afterwards.
    target_grams           INTEGER CHECK (target_grams IS NULL OR target_grams > 0),

    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    issued_at              TEXT    NOT NULL,

    UNIQUE (issued_for, issued_at),

    CHECK ((week_kind = 'climbing') = (week_index IS NOT NULL)),
    CHECK ((anchor_grams IS NULL) = (anchor_provenance IS NULL)),
    CHECK ((anchor_grams IS NULL) = (anchor_from IS NULL)),
    -- A target belongs to a test week and to nothing else.
    CHECK (week_kind = 'test' OR target_grams IS NULL),
    -- And a session derives its primary loads from one of the two, never from
    -- both and never from neither.
    CHECK ((anchor_grams IS NULL) != (target_grams IS NULL))
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

-- Every existing row is a linear programme, so the new columns are null and the
-- old ones carry over untouched.
INSERT INTO programme (id, name, authored_at, template, primary_pattern, primary_exercise, anchor_grams, anchor_provenance, anchor_from, opening_grams, anchor_failed_grams, gating_role, start_date, duration_weeks, test_reps, test_target_grams)
SELECT id, name, authored_at, template, primary_pattern, primary_exercise, anchor_grams, anchor_provenance, anchor_from, opening_grams, anchor_failed_grams, gating_role, start_date, duration_weeks, NULL, NULL
FROM programme_old;

INSERT INTO programme_interruption (programme, start_date, days)
SELECT programme, start_date, days FROM programme_interruption_old;

INSERT INTO programme_slot_fill (programme, slot, role, position, exercise, static_sets, static_reps)
SELECT programme, slot, role, position, exercise, static_sets, static_reps FROM programme_slot_fill_old;

INSERT INTO programme_weekday (programme, weekday, role)
SELECT programme, weekday, role FROM programme_weekday_old;

INSERT INTO prescribed_workout (id, programme, issued_for, zone, session_role, week_kind, week_index, anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams, target_grams, parameters_authored_at, issued_at)
SELECT id, programme, issued_for, zone, session_role, week_kind, week_index, anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams, NULL, parameters_authored_at, issued_at FROM prescribed_workout_old;

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
