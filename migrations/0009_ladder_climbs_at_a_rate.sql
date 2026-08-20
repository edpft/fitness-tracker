-- The linear ladder climbs at a stated rate and has no authored endpoint.
--
-- `ladder_end_bp` said where the climb finishes, and the weekly step was derived
-- by dividing the span by the climbing weeks. The operator settled on 2026-08-19
-- that this is not the model: the linear template picks a starting point and
-- attempts to add a fixed increment every week, and what regulates the climb is
-- the drop-and-re-climb protocol rather than a stated top. So the authored pair
-- becomes a start and a rate. See
-- `docs/decisions/0008-the-linear-ladder-climbs-at-a-rate.md`.
--
-- **The rate is grams, like the two re-climb rates beside it.** A reset is this
-- same climb run at a different rate off a lower start, so `ladder_climb_grams`
-- and `reset1_reclimb_grams` / `reset2_reclimb_grams` are deliberately one kind
-- of thing. A rate expressed as a percentage would land between plates at some
-- anchors and not at others, which is the fault `light_of_heavy` was caught
-- with.
--
-- **Rows already stored take the second reset's rate.** There is no arithmetic
-- that turns a span into a rate — the span was divided by a duration that lives
-- in another table, and reconstructing it would be fitting a parameter, which is
-- the error this whole change is about. What is available instead is a stated
-- identity: `docs/primary-lift-progression.md` says the second reset is "the
-- genuine slowdown: +2.5kg weekly is baseline rate off a lower start". The
-- second reset's re-climb rate *is* the baseline rate, said in prose before this
-- migration needed it, so the conversion reads a document rather than inventing
-- a number.
--
-- **The rebuild reaches eight tables and only one of them changes.** There is no
-- `ALTER TABLE ... DROP COLUMN` for a column named in a CHECK constraint, so
-- `generation_parameters` is rebuilt; renaming it aside re-points every foreign
-- key at the old copy, so every table that transitively references it is rebuilt
-- too. `PRAGMA legacy_alter_table` would avoid that and does not work here: set
-- inside the transaction a sqlx migration runs in, the rename re-points the
-- children anyway. The other seven definitions below are the current schema
-- verbatim — read `0004`, `0006` and `0007` for why their constraints exist.
--
-- The order is `0006`'s, for `0006`'s reasons, which that file states in full:
--
--   1. no indexes or triggers exist on these eight, so none are dropped
--   2. rename every table aside, leaving old referencing old
--   3. create and fill the new tables parents-first
--   4. drop the old tables children-first
--
-- Nothing is deleted while anything points at it, so foreign keys stay on
-- throughout and the whole migration is one transaction.

-- 2. Rename aside. Old now references old.

ALTER TABLE generation_parameters  RENAME TO generation_parameters_old;
ALTER TABLE generation_role_reps   RENAME TO generation_role_reps_old;
ALTER TABLE generation_warmup_step RENAME TO generation_warmup_step_old;
ALTER TABLE prescribed_workout     RENAME TO prescribed_workout_old;
ALTER TABLE prescribed_item        RENAME TO prescribed_item_old;
ALTER TABLE prescribed_slot        RENAME TO prescribed_slot_old;
ALTER TABLE prescribed_exercise    RENAME TO prescribed_exercise_old;
ALTER TABLE prescribed_set         RENAME TO prescribed_set_old;

-- 3. Create and fill, parents-first.

CREATE TABLE generation_parameters (
    authored_at            TEXT    PRIMARY KEY,

    back_off_bp            INTEGER NOT NULL CHECK (back_off_bp > 0),
    light_of_heavy_bp      INTEGER NOT NULL CHECK (light_of_heavy_bp > 0),

    -- Where the climb opens, as a share of the anchor, and what it adds each
    -- climbing week. No endpoint: the climb runs until the calendar stops it.
    ladder_start_bp        INTEGER NOT NULL CHECK (ladder_start_bp > 0),
    ladder_climb_grams     INTEGER NOT NULL CHECK (ladder_climb_grams > 0),

    plate_increment_grams  INTEGER NOT NULL CHECK (plate_increment_grams > 0),

    strength_low           INTEGER NOT NULL CHECK (strength_low > 0),
    strength_high          INTEGER NOT NULL CHECK (strength_high > 0),
    strength_sets          INTEGER NOT NULL CHECK (strength_sets > 0),
    hypertrophy_low        INTEGER NOT NULL CHECK (hypertrophy_low > 0),
    hypertrophy_high       INTEGER NOT NULL CHECK (hypertrophy_high > 0),
    hypertrophy_sets       INTEGER NOT NULL CHECK (hypertrophy_sets > 0),

    static_hold_seconds    INTEGER NOT NULL CHECK (static_hold_seconds > 0),

    reset1_drop_bp         INTEGER NOT NULL CHECK (reset1_drop_bp < 0),
    reset1_reclimb_grams   INTEGER NOT NULL CHECK (reset1_reclimb_grams > 0),
    reset2_drop_bp         INTEGER NOT NULL CHECK (reset2_drop_bp < 0),
    reset2_reclimb_grams   INTEGER NOT NULL CHECK (reset2_reclimb_grams > 0),

    -- `CHECK (ladder_end_bp > ladder_start_bp)` is gone with the column it
    -- guarded. "A ladder that does not rise is not a plan" is now
    -- `ladder_climb_grams > 0`, on the column itself.

    CHECK (strength_high > strength_low),
    CHECK (hypertrophy_high > hypertrophy_low)
) STRICT, WITHOUT ROWID;

INSERT INTO generation_parameters (
    authored_at, back_off_bp, light_of_heavy_bp, ladder_start_bp,
    ladder_climb_grams, plate_increment_grams, strength_low, strength_high,
    strength_sets, hypertrophy_low, hypertrophy_high, hypertrophy_sets,
    static_hold_seconds, reset1_drop_bp, reset1_reclimb_grams,
    reset2_drop_bp, reset2_reclimb_grams
)
SELECT
    authored_at, back_off_bp, light_of_heavy_bp, ladder_start_bp,
    reset2_reclimb_grams, plate_increment_grams, strength_low, strength_high,
    strength_sets, hypertrophy_low, hypertrophy_high, hypertrophy_sets,
    static_hold_seconds, reset1_drop_bp, reset1_reclimb_grams,
    reset2_drop_bp, reset2_reclimb_grams
FROM generation_parameters_old;

CREATE TABLE generation_role_reps (
    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    role                   TEXT    NOT NULL CHECK (role IN ('light', 'heavy')),
    top_set_reps           INTEGER NOT NULL CHECK (top_set_reps > 0),

    PRIMARY KEY (parameters_authored_at, role)
) STRICT, WITHOUT ROWID;

INSERT INTO generation_role_reps (parameters_authored_at, role, top_set_reps)
SELECT parameters_authored_at, role, top_set_reps
FROM generation_role_reps_old;

CREATE TABLE generation_warmup_step (
    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    position               INTEGER NOT NULL,
    of_top_set_bp          INTEGER NOT NULL CHECK (of_top_set_bp > 0),
    reps                   INTEGER NOT NULL CHECK (reps > 0),

    PRIMARY KEY (parameters_authored_at, position)
) STRICT, WITHOUT ROWID;

INSERT INTO generation_warmup_step (
    parameters_authored_at, position, of_top_set_bp, reps
)
SELECT parameters_authored_at, position, of_top_set_bp, reps
FROM generation_warmup_step_old;

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

INSERT INTO prescribed_exercise (
    workout, item_position, position, exercise, measure
)
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

INSERT INTO prescribed_set (
    workout, item_position, exercise_position, position, variant, load_kind,
    load_grams, target_kind, target_low, target_high, effort, rest_low_seconds,
    rest_high_seconds, warmup
)
SELECT
    workout, item_position, exercise_position, position, variant, load_kind,
    load_grams, target_kind, target_low, target_high, effort, rest_low_seconds,
    rest_high_seconds, warmup
FROM prescribed_set_old;

-- 4. Drop the old copies, children-first.

DROP TABLE prescribed_set_old;
DROP TABLE prescribed_exercise_old;
DROP TABLE prescribed_slot_old;
DROP TABLE prescribed_item_old;
DROP TABLE prescribed_workout_old;
DROP TABLE generation_warmup_step_old;
DROP TABLE generation_role_reps_old;
DROP TABLE generation_parameters_old;
