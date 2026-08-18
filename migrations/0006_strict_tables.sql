-- Every table declares its types; this is what makes SQLite enforce them.
--
-- SQLite has five storage classes and no others, and a declared type is only an
-- *affinity* — so `load_grams INTEGER` has been accepting `'banana'` since
-- `0001`, storing it as TEXT, and every CHECK in these files has been guarding
-- values whose type nothing checked. A STRICT table rejects it instead. There is
-- no `ALTER TABLE ... SET STRICT`, so each table is rebuilt.
--
-- **The definitions below are `0001`–`0005`'s, verbatim apart from the trailing
-- `STRICT`.** The reasoning behind every column and every CHECK is in those
-- files and is deliberately not repeated here — a rule stated twice is a rule
-- that drifts. Read them, not this, to learn why a constraint exists; read this
-- to learn what the schema currently is.
--
-- **The order is the point, and it is why no pragma appears.** The documented
-- twelve-step rebuild turns `foreign_keys` off, which is a no-op inside a
-- transaction and therefore a no-op inside a sqlx migration — `0003` did it and
-- got away with it only because `performed_set` is a leaf. `defer_foreign_keys`
-- does work inside a transaction but does not survive this: `DROP TABLE` on a
-- referenced parent counts a deferred violation per row deleted, and recreating
-- the rows under the same name does not discharge the count, so the COMMIT fails
-- with violations that `PRAGMA foreign_key_check` cannot find. So instead:
--
--   1. drop the indexes and triggers, whose names are global and would collide
--   2. rename every table aside, which re-points every foreign key at the old
--      copies, leaving old referencing old and nothing referencing new
--   3. create and fill the new tables parents-first, so every foreign key is
--      satisfiable at the moment its row is written
--   4. drop the old tables children-first, so no parent is ever dropped while a
--      row still references it
--
-- Nothing is deleted while anything points at it, so foreign keys stay on
-- throughout and the whole migration is one transaction.
--
-- **If this migration aborts on a type error, that is the point working.** It
-- means a value in the store does not match the type its column has always
-- declared, and the value is the thing to look at.

-- 1. Indexes and triggers first: their names are global.

DROP INDEX extraction_run_succeeded;
DROP INDEX hevy_workout_landing_latest;
DROP INDEX normalisation_run_succeeded;
DROP INDEX gym_workout_by_source_record;
DROP INDEX normalisation_refusal_by_kind;
DROP INDEX programme_current;

DROP TRIGGER hevy_workout_landing_is_append_only_update;
DROP TRIGGER hevy_workout_landing_is_append_only_delete;

-- 2. Every table aside. Foreign keys follow the rename, so after this the old
--    copies reference each other and nothing references a new table yet.

ALTER TABLE extraction_run RENAME TO extraction_run_old;
ALTER TABLE resumption_point RENAME TO resumption_point_old;
ALTER TABLE normalisation_run RENAME TO normalisation_run_old;
ALTER TABLE hevy_workout_landing RENAME TO hevy_workout_landing_old;
ALTER TABLE gym_workout RENAME TO gym_workout_old;
ALTER TABLE workout_item RENAME TO workout_item_old;
ALTER TABLE performed_exercise RENAME TO performed_exercise_old;
ALTER TABLE performed_set RENAME TO performed_set_old;
ALTER TABLE normalisation_refusal RENAME TO normalisation_refusal_old;
ALTER TABLE generation_parameters RENAME TO generation_parameters_old;
ALTER TABLE generation_warmup_step RENAME TO generation_warmup_step_old;
ALTER TABLE generation_role_reps RENAME TO generation_role_reps_old;
ALTER TABLE programme RENAME TO programme_old;
ALTER TABLE programme_slot_fill RENAME TO programme_slot_fill_old;
ALTER TABLE programme_weekday RENAME TO programme_weekday_old;
ALTER TABLE programme_interruption RENAME TO programme_interruption_old;
ALTER TABLE prescribed_workout RENAME TO prescribed_workout_old;
ALTER TABLE prescribed_item RENAME TO prescribed_item_old;
ALTER TABLE prescribed_slot RENAME TO prescribed_slot_old;
ALTER TABLE prescribed_exercise RENAME TO prescribed_exercise_old;
ALTER TABLE prescribed_set RENAME TO prescribed_set_old;

-- 3. The schema as it now stands, parents first, each filled as it is created.

CREATE TABLE extraction_run (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    stream          TEXT    NOT NULL,
    started_at      TEXT    NOT NULL,
    finished_at     TEXT,
    outcome         TEXT    CHECK (outcome IN ('succeeded', 'failed')),
    events_seen     INTEGER,
    records_landed  INTEGER,
    failure_reason  TEXT,

    CHECK ((outcome IS NULL) = (finished_at IS NULL)),

    CHECK (outcome IS NULL OR (events_seen IS NOT NULL AND records_landed IS NOT NULL)),

    CHECK ((outcome = 'failed') = (failure_reason IS NOT NULL))
) STRICT;

INSERT INTO extraction_run (
    id, stream, started_at, finished_at, outcome, events_seen,
    records_landed, failure_reason
)
SELECT
    id, stream, started_at, finished_at, outcome, events_seen,
    records_landed, failure_reason
FROM extraction_run_old;

CREATE TABLE resumption_point (
    stream      TEXT PRIMARY KEY,
    watermark   TEXT NOT NULL,
    updated_at  TEXT NOT NULL
) STRICT;

INSERT INTO resumption_point (
    stream, watermark, updated_at
)
SELECT
    stream, watermark, updated_at
FROM resumption_point_old;

CREATE TABLE normalisation_run (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    stream               TEXT    NOT NULL,
    started_at           TEXT    NOT NULL,
    finished_at          TEXT,
    outcome              TEXT    CHECK (outcome IN ('succeeded', 'failed')),
    records_read         INTEGER,
    workouts_written     INTEGER,
    workouts_retracted   INTEGER,
    retractions_read     INTEGER,
    records_refused      INTEGER,
    refusals_recorded    INTEGER,
    failure_reason       TEXT,

    CHECK ((outcome IS NULL) = (finished_at IS NULL)),

    CHECK (outcome IS NULL OR (
        records_read IS NOT NULL AND workouts_written IS NOT NULL
        AND workouts_retracted IS NOT NULL AND retractions_read IS NOT NULL
        AND records_refused IS NOT NULL AND refusals_recorded IS NOT NULL
    )),

    CHECK ((outcome = 'failed') = (failure_reason IS NOT NULL))
) STRICT;

INSERT INTO normalisation_run (
    id, stream, started_at, finished_at, outcome, records_read,
    workouts_written, workouts_retracted, retractions_read, records_refused,
    refusals_recorded, failure_reason
)
SELECT
    id, stream, started_at, finished_at, outcome, records_read,
    workouts_written, workouts_retracted, retractions_read, records_refused,
    refusals_recorded, failure_reason
FROM normalisation_run_old;

CREATE TABLE hevy_workout_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,

    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,

    event_kind        TEXT    NOT NULL,

    event_time        TEXT,

    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

INSERT INTO hevy_workout_landing (
    id, endpoint, fetched_at, source_record_id, event_kind, event_time,
    payload, payload_digest, run_id, serve_ordinal
)
SELECT
    id, endpoint, fetched_at, source_record_id, event_kind, event_time,
    payload, payload_digest, run_id, serve_ordinal
FROM hevy_workout_landing_old;

CREATE TABLE gym_workout (
    landing_record_id  INTEGER PRIMARY KEY REFERENCES hevy_workout_landing(id),
    source_record_id   TEXT    NOT NULL,

    started_at_utc     TEXT    NOT NULL,
    zone               TEXT    NOT NULL,

    endpoint           TEXT    NOT NULL,
    event_kind         TEXT    NOT NULL,
    event_time         TEXT,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

INSERT INTO gym_workout (
    landing_record_id, source_record_id, started_at_utc, zone, endpoint,
    event_kind, event_time, run_id
)
SELECT
    landing_record_id, source_record_id, started_at_utc, zone, endpoint,
    event_kind, event_time, run_id
FROM gym_workout_old;

CREATE TABLE workout_item (
    workout       INTEGER NOT NULL REFERENCES gym_workout(landing_record_id),
    position      INTEGER NOT NULL,
    is_superset   INTEGER NOT NULL CHECK (is_superset IN (0, 1)),

    PRIMARY KEY (workout, position)
) STRICT, WITHOUT ROWID;

INSERT INTO workout_item (
    workout, position, is_superset
)
SELECT
    workout, position, is_superset
FROM workout_item_old;

CREATE TABLE performed_exercise (
    workout        INTEGER NOT NULL,
    item_position  INTEGER NOT NULL,

    position       INTEGER NOT NULL,
    exercise       TEXT    NOT NULL,
    measure        TEXT    NOT NULL
        CHECK (measure IN ('reps', 'duration', 'distance')),

    PRIMARY KEY (workout, item_position, position),
    FOREIGN KEY (workout, item_position) REFERENCES workout_item(workout, position)
) STRICT, WITHOUT ROWID;

INSERT INTO performed_exercise (
    workout, item_position, position, exercise, measure
)
SELECT
    workout, item_position, position, exercise, measure
FROM performed_exercise_old;

CREATE TABLE performed_set (
    workout            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    exercise_position  INTEGER NOT NULL,
    position           INTEGER NOT NULL,

    load_kind          TEXT    NOT NULL CHECK (load_kind IN ('absolute', 'relative')),
    load_grams         INTEGER NOT NULL,

    outcome            TEXT    NOT NULL CHECK (outcome IN ('completed', 'failed')),

    reps               INTEGER,
    duration_seconds   INTEGER,
    distance_mm        INTEGER,

    rir                TEXT,
    set_kind           TEXT    NOT NULL CHECK (set_kind IN ('working', 'warmup')),
    rest_after_seconds INTEGER,

    PRIMARY KEY (workout, item_position, exercise_position, position),
    FOREIGN KEY (workout, item_position, exercise_position)
        REFERENCES performed_exercise(workout, item_position, position),

    CHECK (load_kind = 'relative' OR load_grams >= 0),

    CHECK (outcome != 'failed'
           OR (reps IS NULL AND duration_seconds IS NULL AND distance_mm IS NULL)),

    CHECK (outcome != 'completed'
           OR (reps IS NOT NULL) + (duration_seconds IS NOT NULL)
              + (distance_mm IS NOT NULL) >= 1),
    CHECK (reps IS NULL OR (duration_seconds IS NULL AND distance_mm IS NULL)),

    CHECK (reps IS NULL OR reps > 0)
) STRICT, WITHOUT ROWID;

INSERT INTO performed_set (
    workout, item_position, exercise_position, position, load_kind,
    load_grams, outcome, reps, duration_seconds, distance_mm, rir, set_kind,
    rest_after_seconds
)
SELECT
    workout, item_position, exercise_position, position, load_kind,
    load_grams, outcome, reps, duration_seconds, distance_mm, rir, set_kind,
    rest_after_seconds
FROM performed_set_old;

CREATE TABLE normalisation_refusal (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id),
    landing_record_id  INTEGER NOT NULL REFERENCES hevy_workout_landing(id),
    source_record_id   TEXT    NOT NULL,

    locus_kind         TEXT    NOT NULL
        CHECK (locus_kind IN ('record', 'entry', 'set', 'grouping')),
    entry_index        INTEGER,
    set_index          INTEGER,
    group_id           INTEGER,

    exercise           TEXT,

    reason             TEXT    NOT NULL,
    kind               TEXT    NOT NULL
        CHECK (kind IN ('wrong data', 'declared limitation', 'unmodelled')),
    detail             TEXT,

    CHECK (locus_kind != 'record'
           OR (entry_index IS NULL AND set_index IS NULL AND group_id IS NULL)),
    CHECK (locus_kind != 'entry'
           OR (entry_index IS NOT NULL AND set_index IS NULL AND group_id IS NULL)),
    CHECK (locus_kind != 'set'
           OR (entry_index IS NOT NULL AND set_index IS NOT NULL AND group_id IS NULL)),
    CHECK (locus_kind != 'grouping'
           OR (group_id IS NOT NULL AND entry_index IS NULL AND set_index IS NULL))
) STRICT;

INSERT INTO normalisation_refusal (
    id, run_id, landing_record_id, source_record_id, locus_kind,
    entry_index, set_index, group_id, exercise, reason, kind, detail
)
SELECT
    id, run_id, landing_record_id, source_record_id, locus_kind,
    entry_index, set_index, group_id, exercise, reason, kind, detail
FROM normalisation_refusal_old;

CREATE TABLE generation_parameters (
    authored_at            TEXT    PRIMARY KEY,

    back_off_bp            INTEGER NOT NULL CHECK (back_off_bp > 0),
    light_of_heavy_bp      INTEGER NOT NULL CHECK (light_of_heavy_bp > 0),

    ladder_start_bp        INTEGER NOT NULL CHECK (ladder_start_bp > 0),
    ladder_end_bp          INTEGER NOT NULL CHECK (ladder_end_bp > 0),

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

    CHECK (ladder_end_bp > ladder_start_bp),

    CHECK (strength_high > strength_low),
    CHECK (hypertrophy_high > hypertrophy_low)
) STRICT, WITHOUT ROWID;

INSERT INTO generation_parameters (
    authored_at, back_off_bp, light_of_heavy_bp, ladder_start_bp,
    ladder_end_bp, plate_increment_grams, strength_low, strength_high,
    strength_sets, hypertrophy_low, hypertrophy_high, hypertrophy_sets,
    static_hold_seconds, reset1_drop_bp, reset1_reclimb_grams,
    reset2_drop_bp, reset2_reclimb_grams
)
SELECT
    authored_at, back_off_bp, light_of_heavy_bp, ladder_start_bp,
    ladder_end_bp, plate_increment_grams, strength_low, strength_high,
    strength_sets, hypertrophy_low, hypertrophy_high, hypertrophy_sets,
    static_hold_seconds, reset1_drop_bp, reset1_reclimb_grams,
    reset2_drop_bp, reset2_reclimb_grams
FROM generation_parameters_old;

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
SELECT
    parameters_authored_at, position, of_top_set_bp, reps
FROM generation_warmup_step_old;

CREATE TABLE generation_role_reps (
    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    role                   TEXT    NOT NULL CHECK (role IN ('light', 'heavy')),
    top_set_reps           INTEGER NOT NULL CHECK (top_set_reps > 0),

    PRIMARY KEY (parameters_authored_at, role)
) STRICT, WITHOUT ROWID;

INSERT INTO generation_role_reps (
    parameters_authored_at, role, top_set_reps
)
SELECT
    parameters_authored_at, role, top_set_reps
FROM generation_role_reps_old;

CREATE TABLE programme (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at          TEXT    NOT NULL,
    template             TEXT    NOT NULL CHECK (template IN ('v1')),

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
    id, authored_at, template, primary_pattern, primary_exercise,
    anchor_grams, anchor_provenance, anchor_from, gating_role, start_date,
    duration_weeks
FROM programme_old;

-- **The one table that cannot be STRICT**, and it is the nullable `role` in its
-- primary key that stops it: STRICT implies NOT NULL on every primary-key
-- column, and a NULL `role` is what `0004` uses to say "this slot does not
-- alternate by session role". Rebuilt anyway, unchanged, because the rename
-- above re-pointed its foreign key at `programme_old` and this puts it back.
--
-- Its declared types are still unenforced. Closing that needs the data model to
-- stop using NULL as a value, which is a bigger decision than this migration.
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

-- 4. The old copies, children first.

DROP TABLE prescribed_set_old;
DROP TABLE prescribed_exercise_old;
DROP TABLE prescribed_slot_old;
DROP TABLE prescribed_item_old;
DROP TABLE prescribed_workout_old;
DROP TABLE programme_interruption_old;
DROP TABLE programme_weekday_old;
DROP TABLE programme_slot_fill_old;
DROP TABLE programme_old;
DROP TABLE generation_role_reps_old;
DROP TABLE generation_warmup_step_old;
DROP TABLE generation_parameters_old;
DROP TABLE normalisation_refusal_old;
DROP TABLE performed_set_old;
DROP TABLE performed_exercise_old;
DROP TABLE workout_item_old;
DROP TABLE gym_workout_old;
DROP TABLE hevy_workout_landing_old;
DROP TABLE normalisation_run_old;
DROP TABLE resumption_point_old;
DROP TABLE extraction_run_old;

-- The indexes and triggers, unchanged from where they were declared.

CREATE INDEX extraction_run_succeeded
    ON extraction_run (stream, finished_at DESC)
    WHERE outcome = 'succeeded';

CREATE INDEX hevy_workout_landing_latest
    ON hevy_workout_landing (source_record_id, id DESC);

CREATE INDEX normalisation_run_succeeded
    ON normalisation_run (stream, finished_at DESC)
    WHERE outcome = 'succeeded';

CREATE INDEX gym_workout_by_source_record
    ON gym_workout (source_record_id);

CREATE INDEX normalisation_refusal_by_kind
    ON normalisation_refusal (kind, reason);

CREATE INDEX programme_current ON programme (authored_at DESC);

CREATE TRIGGER hevy_workout_landing_is_append_only_update
BEFORE UPDATE ON hevy_workout_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER hevy_workout_landing_is_append_only_delete
BEFORE DELETE ON hevy_workout_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;
