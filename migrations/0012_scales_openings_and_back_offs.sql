-- The opening is declared or dropped, the back-off is per role, and an
-- increment is a scale.
--
-- Three changes the operator settled on 2026-08-20, and one new table.
--
-- **The ladder opens below the failed load, not at it.** `0010` had a block open
-- *at* what its entry test failed and climb in to it, which made week one heavier
-- than the anchor. It now opens at that load dropped by `entry_drop_bp` and climbs
-- through it, so the climb-in is gone and every re-climb in `progression` is a
-- stall. `docs/decisions/0009-a-linear-block-opens-from-its-entry-test.md` is
-- amended rather than withdrawn: the block still opens from its entry test.
--
-- **And a block may state its opening outright.** `programme.opening_grams`, null
-- to derive. The derivation reads a test, and a test far enough behind the block
-- is not evidence about it whatever number it produces — this block's 85kg is
-- reproduced by −10% off the failed 95 *and* by −5% off the completed 90, which is
-- two rules agreeing on one observation and therefore neither being evidenced.
--
-- **The back-off moves to `generation_role_reps`,** which was already per role.
-- `back_off_bp` moves with it and gains sets and reps of its own.
--
-- **`plate_increment_grams` becomes `generation_load_scale`.** One increment for
-- every implement prescribed a dumbbell at 12.5kg and another at 9.5kg. A scale is
-- banded — whole kilos to 10kg, twos above it — and is per implement, so a rack
-- and a bar stop sharing a number.
--
-- **What the historical row is filled with, and why each fill is true.** One
-- parameter set exists and one prescription references it.
--
--   entry_drop_bp     from reset2_drop_bp. The retired climb-in ran at the second
--                     reset's drop, so this is what that parameter set actually
--                     dropped by on entry. Not a guess: a restatement.
--   back_off_*        from back_off_bp, strength_sets and strength_high, which is
--                     exactly where the old code read the primary's back-off.
--   load scale        one `barbell` band from plate_increment_grams. The retired
--                     increment was applied to every implement; that it described
--                     the barbell is the one claim it made that is still true, and
--                     propagating it to the rack would re-author the bug.
--   opening_grams     null. That programme derived its opening and did not state
--                     one.
--
-- Twelve tables are rebuilt for `0006`'s reasons, in `0006`'s order, which that
-- file states in full: renaming a table re-points every foreign key at the old
-- copy, so a table that changes takes its whole reference graph with it.
--
--   1. no indexes or triggers exist on these twelve, so none are dropped
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
ALTER TABLE programme              RENAME TO programme_old;
ALTER TABLE programme_interruption RENAME TO programme_interruption_old;
ALTER TABLE programme_slot_fill    RENAME TO programme_slot_fill_old;
ALTER TABLE programme_weekday      RENAME TO programme_weekday_old;
ALTER TABLE prescribed_workout     RENAME TO prescribed_workout_old;
ALTER TABLE prescribed_item        RENAME TO prescribed_item_old;
ALTER TABLE prescribed_slot        RENAME TO prescribed_slot_old;
ALTER TABLE prescribed_exercise    RENAME TO prescribed_exercise_old;
ALTER TABLE prescribed_set         RENAME TO prescribed_set_old;

-- 3. Create and fill, parents-first.

CREATE TABLE generation_parameters (
    authored_at            TEXT    PRIMARY KEY,

    light_of_heavy_bp      INTEGER NOT NULL CHECK (light_of_heavy_bp > 0),

    -- Where the climb opens, as a share of the anchor, and what it adds each
    -- climbing week. No endpoint: the climb runs until the calendar stops it.
    ladder_climb_grams     INTEGER NOT NULL CHECK (ladder_climb_grams > 0),

    -- What a derived opening drops off the load the entry test failed.
    -- Negative. Authored in its own right rather than read off reset 1, whose
    -- drop it happens to equal: two values agreeing by decision is not the same
    -- as one value used twice, and only the first survives either being changed.
    entry_drop_bp          INTEGER NOT NULL CHECK (entry_drop_bp < 0),


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

INSERT INTO generation_parameters (authored_at, light_of_heavy_bp, ladder_climb_grams, strength_low, strength_high, strength_sets, hypertrophy_low, hypertrophy_high, hypertrophy_sets, static_hold_seconds, reset1_drop_bp, reset1_reclimb_grams, reset2_drop_bp, reset2_reclimb_grams, entry_drop_bp)
SELECT authored_at, light_of_heavy_bp, ladder_climb_grams, strength_low, strength_high, strength_sets, hypertrophy_low, hypertrophy_high, hypertrophy_sets, static_hold_seconds, reset1_drop_bp, reset1_reclimb_grams, reset2_drop_bp, reset2_reclimb_grams, reset2_drop_bp
FROM generation_parameters_old;

-- One implement's load scale, as bands of step size.
--
-- Bands rather than a number, because a dumbbell rack is not a bar: ours moves
-- in whole kilos to 10kg and in twos above it, so "the increment" has no single
-- value. `from_grams` is the lightest load the band applies to and the first
-- band of every scale must start at nothing, which is what makes "the step at
-- this load" total. The domain validates that on the way in; the CHECKs below
-- catch a row this program did not write.
--
-- An implement absent from here is not defaulted to the barbell's steps.
-- Anything loaded on it reports as underivable and the rest of the session
-- still issues, because a prescription derived from an invented grid looks
-- exactly like one derived from the gym's real equipment.
CREATE TABLE generation_load_scale (
    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    implement              TEXT    NOT NULL
        CHECK (implement IN ('barbell', 'dumbbell', 'kettlebell', 'cable',
                             'machine', 'band', 'plate', 'sled', 'bodyweight')),
    band                   INTEGER NOT NULL CHECK (band >= 0),

    from_grams             INTEGER NOT NULL CHECK (from_grams >= 0),
    size_grams             INTEGER NOT NULL CHECK (size_grams > 0),

    -- The first band starts at nothing, so no load falls outside the scale.
    CHECK (band > 0 OR from_grams = 0),

    PRIMARY KEY (parameters_authored_at, implement, band)
) STRICT, WITHOUT ROWID;

INSERT INTO generation_load_scale (
    parameters_authored_at, implement, band, from_grams, size_grams
)
SELECT authored_at, 'barbell', 0, 0, plate_increment_grams
FROM generation_parameters_old;

CREATE TABLE generation_role_reps (
    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    role                   TEXT    NOT NULL CHECK (role IN ('light', 'heavy')),
    top_set_reps           INTEGER NOT NULL CHECK (top_set_reps > 0),

    -- The primary's back-off sets for this role. Here rather than on
    -- `generation_parameters` because the two roles differ: heavy is 2 x 4 and
    -- light is 3 x 6, which the operator stated on 2026-08-20. They used to be
    -- read off `strength_sets` and `strength_high`, which issued the light
    -- session's pattern on the heavy day.
    back_off_sets          INTEGER NOT NULL CHECK (back_off_sets > 0),
    back_off_reps          INTEGER NOT NULL CHECK (back_off_reps > 0),
    back_off_bp            INTEGER NOT NULL CHECK (back_off_bp > 0),

    PRIMARY KEY (parameters_authored_at, role)
) STRICT, WITHOUT ROWID;

INSERT INTO generation_role_reps (
    parameters_authored_at, role, top_set_reps,
    back_off_sets, back_off_reps, back_off_bp
)
SELECT
    r.parameters_authored_at, r.role, r.top_set_reps,
    p.strength_sets, p.strength_high, p.back_off_bp
FROM generation_role_reps_old r
JOIN generation_parameters_old p
  ON p.authored_at = r.parameters_authored_at;

CREATE TABLE generation_warmup_step (
    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    position               INTEGER NOT NULL,
    of_top_set_bp          INTEGER NOT NULL CHECK (of_top_set_bp > 0),
    reps                   INTEGER NOT NULL CHECK (reps > 0),

    PRIMARY KEY (parameters_authored_at, position)
) STRICT, WITHOUT ROWID;

INSERT INTO generation_warmup_step (parameters_authored_at, position, of_top_set_bp, reps)
SELECT parameters_authored_at, position, of_top_set_bp, reps
FROM generation_warmup_step_old;

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

    duration_weeks       INTEGER NOT NULL CHECK (duration_weeks >= 2)
) STRICT;

INSERT INTO programme (id, authored_at, template, primary_pattern, primary_exercise, anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams, gating_role, start_date, duration_weeks)
SELECT id, authored_at, template, primary_pattern, primary_exercise, anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams, gating_role, start_date, duration_weeks
FROM programme_old;

CREATE TABLE programme_interruption (
    programme INTEGER NOT NULL REFERENCES programme(id),

    week      TEXT    NOT NULL,

    PRIMARY KEY (programme, week)
) STRICT, WITHOUT ROWID;

INSERT INTO programme_interruption (programme, week)
SELECT programme, week
FROM programme_interruption_old;

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

INSERT INTO programme_slot_fill (programme, slot, role, position, exercise, static_sets, static_reps)
SELECT programme, slot, role, position, exercise, static_sets, static_reps
FROM programme_slot_fill_old;

CREATE TABLE programme_weekday (
    programme  INTEGER NOT NULL REFERENCES programme(id),
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    role       TEXT    NOT NULL CHECK (role IN ('light', 'heavy')),

    PRIMARY KEY (programme, weekday)
) STRICT, WITHOUT ROWID;

INSERT INTO programme_weekday (programme, weekday, role)
SELECT programme, weekday, role
FROM programme_weekday_old;

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

    UNIQUE (issued_for),

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
DROP TABLE programme_weekday_old;
DROP TABLE programme_slot_fill_old;
DROP TABLE programme_interruption_old;
DROP TABLE programme_old;
DROP TABLE generation_warmup_step_old;
DROP TABLE generation_role_reps_old;
DROP TABLE generation_parameters_old;
