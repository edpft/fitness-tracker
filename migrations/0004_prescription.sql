-- The prescribed side of § 11, and the authored data it is generated from.
--
-- Everything here is § III data, and what is absent says so. There are no
-- append-only triggers, because those guard raw (§ II.1) and none of this is an
-- observation. There is no wholesale-replacement path either, because that is
-- how a *derivation* stays honest (§ II) and none of this is derived: nothing
-- regenerates an authored programme or an issued prescription if it is lost.
--
-- So these tables are written once and kept. Supersession is by `authored_at`
-- rather than by a mutable `is_current` flag — "the one in force" is a `WHERE`
-- clause, for the same reason the normalised layer has no such column.
--
-- What is deliberately not here: a table for a `WorkoutShape` on its own. A
-- shape only reaches storage as part of a prescription that was issued, so a
-- shape projected out of a performed workout has nowhere to be written. That is
-- FR-034 held at the schema as well as in the types.

-- Generation parameters (§ 14). Only the current value is required, because
-- what they produced is recorded concretely on the prescription that used them.
-- Superseded rows are kept anyway: they cost nothing, and an issued
-- prescription names the version it read.
CREATE TABLE generation_parameters (
    authored_at            TEXT    PRIMARY KEY,

    -- Percentages are integer basis points. A float would not round-trip, and a
    -- stored prescription that cannot be reproduced is not a record of anything.
    back_off_bp            INTEGER NOT NULL CHECK (back_off_bp > 0),
    light_of_heavy_bp      INTEGER NOT NULL CHECK (light_of_heavy_bp > 0),

    -- The ladder's span. The weekly step is derived from these and the
    -- programme's duration, never stored: an endpoint is a claim about
    -- achievable gain, a step is a number with nothing behind it.
    ladder_start_bp        INTEGER NOT NULL CHECK (ladder_start_bp > 0),
    ladder_end_bp          INTEGER NOT NULL CHECK (ladder_end_bp > 0),

    plate_increment_grams  INTEGER NOT NULL CHECK (plate_increment_grams > 0),

    -- The double-progression scheme every non-primary strength and hypertrophy
    -- slot runs. One range for all of them is a simplification the domain
    -- records; a per-slot range would be more faithful and is deferred.
    accessory_low          INTEGER NOT NULL CHECK (accessory_low > 0),
    accessory_high         INTEGER NOT NULL CHECK (accessory_high > 0),
    accessory_sets         INTEGER NOT NULL CHECK (accessory_sets > 0),

    -- How long a static hold is held for. The mobility work does not progress,
    -- so its prescription comes from here rather than from observed history.
    static_hold_seconds    INTEGER NOT NULL CHECK (static_hold_seconds > 0),

    -- From `docs/primary-lift-progression.md`. Drops are negative.
    reset1_drop_bp         INTEGER NOT NULL CHECK (reset1_drop_bp < 0),
    reset1_reclimb_grams   INTEGER NOT NULL CHECK (reset1_reclimb_grams > 0),
    reset2_drop_bp         INTEGER NOT NULL CHECK (reset2_drop_bp < 0),
    reset2_reclimb_grams   INTEGER NOT NULL CHECK (reset2_reclimb_grams > 0),

    -- A ladder that does not rise is not a plan.
    CHECK (ladder_end_bp > ladder_start_bp),
    -- A range must span; equal bounds would be a fixed count.
    CHECK (accessory_high > accessory_low)
) WITHOUT ROWID;

CREATE TABLE generation_warmup_step (
    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    position               INTEGER NOT NULL,
    of_top_set_bp          INTEGER NOT NULL CHECK (of_top_set_bp > 0),
    reps                   INTEGER NOT NULL CHECK (reps > 0),

    PRIMARY KEY (parameters_authored_at, position)
) WITHOUT ROWID;

-- Repetitions per session role. Both roles are always present; `PerRole` is a
-- struct rather than a map, so a missing role is unrepresentable in Rust and
-- this table is where that has to be asserted instead.
CREATE TABLE generation_role_reps (
    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    role                   TEXT    NOT NULL CHECK (role IN ('light', 'heavy')),
    top_set_reps           INTEGER NOT NULL CHECK (top_set_reps > 0),

    PRIMARY KEY (parameters_authored_at, role)
) WITHOUT ROWID;

-- The programme. Authored intent (§ 12).
CREATE TABLE programme (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at          TEXT    NOT NULL,
    template             TEXT    NOT NULL CHECK (template IN ('v1')),

    primary_pattern      TEXT    NOT NULL
        CHECK (primary_pattern IN ('knee_dominant', 'hip_dominant',
                                   'upper_push', 'upper_pull')),
    primary_exercise     TEXT    NOT NULL,

    -- The starting 1RM. Fixed for the block; only the exit test replaces it,
    -- and that replacement anchors the *next* block.
    anchor_grams         INTEGER NOT NULL CHECK (anchor_grams > 0),
    anchor_provenance    TEXT    NOT NULL
        CHECK (anchor_provenance IN ('tested', 'estimated', 'asserted')),
    anchor_from          TEXT    NOT NULL,

    gating_role          TEXT    NOT NULL CHECK (gating_role IN ('light', 'heavy')),
    start_date           TEXT    NOT NULL,
    -- Weeks, not cycles. The last one is the test, so a block needs at least
    -- one climbing week besides it.
    duration_weeks       INTEGER NOT NULL CHECK (duration_weeks >= 2)
);

CREATE INDEX programme_current ON programme (authored_at DESC);

-- One fill per slot. A slot that alternates by session role has one row per
-- role; a slot that does not has a single row with `role` NULL.
CREATE TABLE programme_slot_fill (
    programme  INTEGER NOT NULL REFERENCES programme(id),
    slot       TEXT    NOT NULL,
    role       TEXT    CHECK (role IS NULL OR role IN ('light', 'heavy')),
    -- Position within a supersetted slot; 0 for a single one.
    position   INTEGER NOT NULL DEFAULT 0,
    exercise   TEXT    NOT NULL,

    PRIMARY KEY (programme, slot, role, position)
);

CREATE TABLE programme_weekday (
    programme  INTEGER NOT NULL REFERENCES programme(id),
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    role       TEXT    NOT NULL CHECK (role IN ('light', 'heavy')),

    PRIMARY KEY (programme, weekday)
) WITHOUT ROWID;

-- What was issued. Written once, never rewritten (§ 12).
--
-- The anchor and the parameter version are recorded by value, which is what
-- makes § 14's "only the current value is required" true: a superseded
-- percentage answers no question because what it produced is here.
CREATE TABLE prescribed_workout (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    programme              INTEGER NOT NULL REFERENCES programme(id),

    -- A date, not an instant. Also the join key a later correspondence feature
    -- will use; designing without it would make correspondence a migration.
    issued_for             TEXT    NOT NULL,
    zone                   TEXT    NOT NULL,
    session_role           TEXT    NOT NULL CHECK (session_role IN ('light', 'heavy')),

    -- A ladder position, or the block's test. `WeekKind` projected.
    week_kind              TEXT    NOT NULL CHECK (week_kind IN ('climbing', 'test')),
    week_index             INTEGER,

    anchor_grams           INTEGER NOT NULL CHECK (anchor_grams > 0),
    anchor_provenance      TEXT    NOT NULL
        CHECK (anchor_provenance IN ('tested', 'estimated', 'asserted')),
    anchor_from            TEXT    NOT NULL,

    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    issued_at              TEXT    NOT NULL,

    -- One prescription per date. FR-010's idempotence, held by the schema
    -- rather than by a caller remembering to check.
    UNIQUE (issued_for),

    CHECK ((week_kind = 'climbing') = (week_index IS NOT NULL))
);

CREATE TABLE prescribed_item (
    workout      INTEGER NOT NULL REFERENCES prescribed_workout(id),
    position     INTEGER NOT NULL,
    is_superset  INTEGER NOT NULL CHECK (is_superset IN (0, 1)),

    PRIMARY KEY (workout, position)
) WITHOUT ROWID;

-- Items are slot-tagged, or "same slot, different cycle" stops being
-- answerable — and that comparability was the argument for slots existing. A
-- superset tags each member, hence a row per member rather than per item.
CREATE TABLE prescribed_slot (
    workout          INTEGER NOT NULL,
    item_position    INTEGER NOT NULL,
    member_position  INTEGER NOT NULL,
    slot             TEXT    NOT NULL,

    PRIMARY KEY (workout, item_position, member_position),
    FOREIGN KEY (workout, item_position)
        REFERENCES prescribed_item(workout, position)
) WITHOUT ROWID;

CREATE TABLE prescribed_exercise (
    workout        INTEGER NOT NULL,
    item_position  INTEGER NOT NULL,
    position       INTEGER NOT NULL,
    exercise       TEXT    NOT NULL,
    measure        TEXT    NOT NULL CHECK (measure IN ('reps', 'duration', 'distance')),

    PRIMARY KEY (workout, item_position, position),
    FOREIGN KEY (workout, item_position)
        REFERENCES prescribed_item(workout, position)
) WITHOUT ROWID;

-- `Prescribed<M>` projected. The variant decides which columns mean anything,
-- and the CHECKs below make "prescribes nothing" unrepresentable in the file as
-- well as in Rust — including to a writer that is not this program.
CREATE TABLE prescribed_set (
    workout            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    exercise_position  INTEGER NOT NULL,
    position           INTEGER NOT NULL,

    variant            TEXT    NOT NULL
        CHECK (variant IN ('fixed', 'to_effort', 'autoregulated')),

    load_kind          TEXT    CHECK (load_kind IN ('absolute', 'relative')),
    load_grams         INTEGER,

    -- `Target<M>` flattened. `target_high` NULL means `Exactly`.
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

    -- Load and measure pinned; effort optional.
    CHECK (variant != 'fixed'
           OR (load_kind IS NOT NULL AND target_kind IS NOT NULL)),
    -- Measure open; load and effort bind. `target_*` may still carry a
    -- prediction, which is typed apart from a prescription on purpose.
    CHECK (variant != 'to_effort'
           OR (load_kind IS NOT NULL AND effort IS NOT NULL)),
    -- Load open; measure pinned; effort binds.
    CHECK (variant != 'autoregulated'
           OR (load_kind IS NULL AND target_kind IS NOT NULL AND effort IS NOT NULL)),

    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK ((target_kind IS NULL) = (target_low IS NULL)),
    CHECK (target_high IS NULL OR target_low IS NOT NULL),
    -- A range that does not span is an `Exactly`, and there is no third state.
    CHECK (target_high IS NULL OR target_high > target_low),
    CHECK ((rest_high_seconds IS NULL) OR (rest_low_seconds IS NOT NULL)),
    CHECK (rest_high_seconds IS NULL OR rest_high_seconds > rest_low_seconds)
) WITHOUT ROWID;
