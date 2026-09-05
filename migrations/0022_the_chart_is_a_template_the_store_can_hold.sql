-- The chart is a template the store can hold.
--
-- `Sbs` has existed in `domain` since decision 0024, the document reader accepts
-- `template = "sbs"`, and `sbs_load` prescribes from it -- but `programme`'s
-- `CHECK` was written in 0016, when there were three templates, and an SBS cycle
-- has been unstorable ever since. The autumn block cannot be authored at all
-- until this lands, which is the only reason it exists.
--
-- **One value added, and two rules that follow from it.**
--
-- `opening_grams` joins a test's in being refused, because every load in the
-- chart is a share of a maximum that moves inside the cycle: there is no rung to
-- open at, and `an_opening_is_refused_because_every_load_is_a_share` already
-- refuses one at the door.
--
-- `duration_weeks` is pinned to four rather than left at "two or more". The
-- chart is four weeks, so a five-week SBS cycle is not this programme run
-- longer -- it is a different programme, with no rule for what its extra weeks
-- would prescribe. `Sbs::new` refuses one and now the row cannot hold one.
--
-- Nothing else about the row shape changes. An SBS cycle carries an anchor like
-- any other climbing programme, reports `heavy` as its gating role because
-- `sbs::programme::GATING` says so, and states no test repetitions of its own.
--
-- **Ten tables, in 0016's order and for its reason.** SQLite rewrites a child's
-- `REFERENCES` clause when its parent is renamed, so rebuilding `programme`
-- alone would leave nine tables pointing at `programme_old`. The graph is
-- rebuilt whole. It is one table longer than 0016's because
-- `prescription_delivery` arrived in 0017.

-- Rename, parents first.

ALTER TABLE programme              RENAME TO programme_old;
ALTER TABLE programme_interruption RENAME TO programme_interruption_old;
ALTER TABLE programme_slot_fill    RENAME TO programme_slot_fill_old;
ALTER TABLE programme_weekday      RENAME TO programme_weekday_old;
ALTER TABLE prescribed_workout     RENAME TO prescribed_workout_old;
ALTER TABLE prescribed_item        RENAME TO prescribed_item_old;
ALTER TABLE prescribed_slot        RENAME TO prescribed_slot_old;
ALTER TABLE prescribed_exercise    RENAME TO prescribed_exercise_old;
ALTER TABLE prescribed_set         RENAME TO prescribed_set_old;
ALTER TABLE prescription_delivery  RENAME TO prescription_delivery_old;

-- Recreate, with  admitted.

CREATE TABLE programme (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    name                 TEXT    NOT NULL,

    authored_at          TEXT    NOT NULL,
    -- 'v1' until 2026-08-18, when linear and block stopped being versions of one
    -- thing; 'test' from 2026-08-22, when a test stopped being a week of one.
    template             TEXT    NOT NULL
        CHECK (template IN ('linear', 'block', 'sbs', 'test')),

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

    -- A block's entry test: the week it spends measuring what it plans from.
    --
    -- Null is a block that opens from a test which already happened, and its
    -- anchor must then say 'tested'. Non-null is a block that measures its own,
    -- so the anchor is what the operator expects and the week finds out.
    --
    -- `entry_test_light_grams` is what the week's other session runs its primary
    -- at, and null means it is not run: the lift's maximum is what the week is
    -- about to measure, so there is nothing to derive a light load from and the
    -- operator states one or trains once that week.
    entry_test_reps      INTEGER CHECK (entry_test_reps IS NULL OR entry_test_reps > 0),
    entry_test_light_grams INTEGER CHECK (entry_test_light_grams IS NULL
                                          OR entry_test_light_grams > 0),

    -- A test has no anchor, no opening and no gate; every other template has an
    -- anchor and a gate, and may have an opening.
    CHECK ((template = 'test') = (anchor_grams IS NULL)),
    CHECK ((anchor_grams IS NULL) = (anchor_provenance IS NULL)),
    CHECK ((anchor_grams IS NULL) = (anchor_from IS NULL)),
    CHECK ((template = 'test') = (gating_role IS NULL)),
    CHECK (template NOT IN ('test', 'sbs') OR opening_grams IS NULL),

    -- And only a test has a repetition count or a target.
    CHECK ((template = 'test') = (test_reps IS NOT NULL)),
    CHECK (template = 'test' OR test_target_grams IS NULL),

    -- A test is one week. Anything that climbs needs at least two, because a
    -- ladder of one rung is a load rather than a plan.
    CHECK (CASE template WHEN 'test' THEN duration_weeks = 1
                         WHEN 'sbs'  THEN duration_weeks = 4
                         ELSE duration_weeks >= 2 END),

    -- Only a block has an entry test, and a light load without one is a load
    -- for a session that does not exist.
    CHECK (template = 'block' OR entry_test_reps IS NULL),
    CHECK (entry_test_reps IS NOT NULL OR entry_test_light_grams IS NULL),

    -- **Nothing here says a block's anchor must have been measured**, and that
    -- is deliberate. Whether it had to be depends on what precedes the block —
    -- nothing at all, a test in the wrong lift, or a measurement it should have
    -- opened from — and no `CHECK` can see another row's programme, let alone
    -- decide which of them is the one immediately before. The rule lives in
    -- `Authoring`, which can ask.

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

CREATE TABLE prescription_delivery (
    prescription INTEGER NOT NULL REFERENCES prescribed_workout(id),

    -- Ours, not the destination's. Lowercase and without whitespace, the rules
    -- every name we assign answers to.
    destination  TEXT    NOT NULL
        CHECK (destination <> '' AND destination = lower(destination)),

    -- What the destination called the session. Opaque: never parsed, never
    -- compared to anything but another of its own kind, and constrained only
    -- against being empty — the value belongs to the system that issued it.
    reference    TEXT    NOT NULL CHECK (reference <> ''),

    delivered_at TEXT    NOT NULL,

    PRIMARY KEY (prescription, destination)
) STRICT, WITHOUT ROWID;

-- Copy, parents first, column by column so a reordering cannot go unnoticed.

INSERT INTO programme (id, name, authored_at, template, primary_pattern, primary_exercise, anchor_grams, anchor_provenance, anchor_from, opening_grams, anchor_failed_grams, gating_role, start_date, duration_weeks, test_reps, test_target_grams, entry_test_reps, entry_test_light_grams)
    SELECT id, name, authored_at, template, primary_pattern, primary_exercise, anchor_grams, anchor_provenance, anchor_from, opening_grams, anchor_failed_grams, gating_role, start_date, duration_weeks, test_reps, test_target_grams, entry_test_reps, entry_test_light_grams FROM programme_old;

INSERT INTO programme_interruption (programme, start_date, days)
    SELECT programme, start_date, days FROM programme_interruption_old;

INSERT INTO programme_slot_fill (programme, slot, role, position, exercise, static_sets, static_reps)
    SELECT programme, slot, role, position, exercise, static_sets, static_reps FROM programme_slot_fill_old;

INSERT INTO programme_weekday (programme, weekday, role)
    SELECT programme, weekday, role FROM programme_weekday_old;

INSERT INTO prescribed_workout (id, programme, issued_for, zone, session_role, week_kind, week_index, anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams, target_grams, parameters_authored_at, issued_at)
    SELECT id, programme, issued_for, zone, session_role, week_kind, week_index, anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams, target_grams, parameters_authored_at, issued_at FROM prescribed_workout_old;

INSERT INTO prescribed_item (workout, position, is_superset)
    SELECT workout, position, is_superset FROM prescribed_item_old;

INSERT INTO prescribed_slot (workout, item_position, member_position, slot)
    SELECT workout, item_position, member_position, slot FROM prescribed_slot_old;

INSERT INTO prescribed_exercise (workout, item_position, position, exercise, measure)
    SELECT workout, item_position, position, exercise, measure FROM prescribed_exercise_old;

INSERT INTO prescribed_set (workout, item_position, exercise_position, position, variant, load_kind, load_grams, target_kind, target_low, target_high, effort, rest_low_seconds, rest_high_seconds, warmup)
    SELECT workout, item_position, exercise_position, position, variant, load_kind, load_grams, target_kind, target_low, target_high, effort, rest_low_seconds, rest_high_seconds, warmup FROM prescribed_set_old;

INSERT INTO prescription_delivery (prescription, destination, reference, delivered_at)
    SELECT prescription, destination, reference, delivered_at FROM prescription_delivery_old;

-- Drop, children first.

DROP TABLE prescription_delivery_old;
DROP TABLE prescribed_set_old;
DROP TABLE prescribed_exercise_old;
DROP TABLE prescribed_slot_old;
DROP TABLE prescribed_item_old;
DROP TABLE prescribed_workout_old;
DROP TABLE programme_weekday_old;
DROP TABLE programme_slot_fill_old;
DROP TABLE programme_interruption_old;
DROP TABLE programme_old;

-- The triggers moved with their tables and were dropped with them.

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

CREATE TRIGGER prescription_delivery_performed_is_not_deletable
BEFORE DELETE ON prescription_delivery
WHEN EXISTS (
    SELECT 1 FROM gym_workout WHERE performed_against = OLD.reference
)
BEGIN
    SELECT RAISE(ABORT, 'a performed session is not withdrawable (constitution 12)');
END;
