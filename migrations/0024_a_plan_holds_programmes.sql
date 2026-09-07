-- A plan holds programmes, and a programme holds its mesocycles (issue #86).
--
-- The operator's hierarchy, 2026-09-06:
--
--     macrocycle -> plan -> programme -> mesocycle -> microcycle -> session
--
-- The autumn is **one plan**. It holds a cycling programme and a gym programme,
-- and each of those holds four mesocycles: one entry test and three
-- progressions. Until now the store had four rows per discipline and no plan at
-- all, because #55 made the *mesocycle* the authored unit and leant on
-- succession -- successive start dates and one name each -- to hold them
-- together. "The autumn plan" existed nowhere.
--
-- **What each table is called now says which rung it is on.** `programme` held a
-- mesocycle, so it is `gym_mesocycle`; `cycling_programme` held one too, so it
-- is `cycling_mesocycle`. Their children take the `gym_` and `cycling_` prefixes
-- their siblings already had, and the foreign key column is `mesocycle` rather
-- than `programme` in every one of them.
--
-- **There is no table for the programme rung**, and that is deliberate. A row
-- holding a plan id and a discipline that the table name already states would
-- carry nothing: the gym programme *is* the `gym_mesocycle` rows under a plan,
-- in `ordinal` order.
--
-- **The rows go rather than being carried.** The operator, 2026-09-06:
--
-- > "local.db has only been used for beta testing, that's why it contains so
-- > many prescriptions. store.db will hold the autumn plan. at this point,
-- > there's nothing that can't be reconstructed after the fact."
--
-- So this drops and rebuilds instead of translating. Landing, normalisation, the
-- schedule and the generation parameters are untouched: what goes is the
-- authored side and the prescriptions issued from it, and both are re-authored
-- in minutes. **This stops being true once the autumn is running** (constitution
-- 12): a migration after 14 September carries its rows or does not land.
--
-- **`name` comes off a mesocycle, and `authored_at` with it.** Identity is the
-- plan's -- one authoring covers every mesocycle in it, and re-authoring the
-- plan supersedes the lot. A per-mesocycle timestamp would be a second source of
-- truth for a fact the plan carries, and the two could disagree about which
-- version is current. That is the argument `cycling_programme` already made for
-- not storing its own duration.
--
-- **Both disciplines record what provided a mesocycle, in the same two
-- columns.** The gym side recorded nothing at all -- it stored
-- `template = 'sbs'`, which names the publisher of one chart rather than the
-- programme -- and the cycling side recorded the programme once per microcycle,
-- which was one fact written many times and free to disagree with itself.
--
-- **Dropped leaves first, and `cycling_interval` is a leaf.** With foreign keys
-- enforced, SQLite compiles a parent's implicit delete against every table that
-- still declares a reference into the graph, so an order that leaves one behind
-- fails on a table it cannot resolve rather than on a constraint. Migration 0022
-- never met this because the gym tables reference only their parent; the cycling
-- ones reference each other.

DROP TRIGGER IF EXISTS prescription_delivery_performed_is_not_deletable;
DROP TRIGGER IF EXISTS prescribed_workout_performed_is_not_deletable;

DROP TABLE IF EXISTS prescription_delivery;
DROP TABLE IF EXISTS prescribed_set;
DROP TABLE IF EXISTS prescribed_exercise;
DROP TABLE IF EXISTS prescribed_slot;
DROP TABLE IF EXISTS prescribed_item;
DROP TABLE IF EXISTS prescribed_workout;

DROP TABLE IF EXISTS cycling_interval;
DROP TABLE IF EXISTS cycling_venue;
DROP TABLE IF EXISTS cycling_ride;
DROP TABLE IF EXISTS cycling_microcycle;
DROP TABLE IF EXISTS cycling_weekday;
DROP TABLE IF EXISTS cycling_programme;

DROP TABLE IF EXISTS programme_weekday;
DROP TABLE IF EXISTS programme_slot_fill;
DROP TABLE IF EXISTS programme_interruption;
DROP TABLE IF EXISTS programme;

-- The authored unit. One row per authoring; the plan in force under a name is
-- its latest, which is `MAX(authored_at)` per name exactly as a programme's was.
--
-- **No start date and no duration.** A plan runs from the earliest day its
-- mesocycles occupy to the last, so a span here would be a second source of
-- truth for a fact those rows already carry between them.
CREATE TABLE plan (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,

    -- The operator's own label, and the identity a re-authoring supersedes on.
    name         TEXT    NOT NULL CHECK (length(trim(name)) > 0),
    authored_at  TEXT    NOT NULL,

    UNIQUE (name, authored_at)
) STRICT;

CREATE TABLE gym_mesocycle (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,

    -- The plan this belongs to, and where in that plan's programme for this
    -- discipline it sits. **Identity is the plan's**: re-authoring a plan
    -- supersedes every mesocycle in it at once, which is what a per-mesocycle
    -- `name` used to do one level too low.
    plan               INTEGER NOT NULL REFERENCES plan(id) ON DELETE CASCADE,
    ordinal            INTEGER NOT NULL CHECK (ordinal > 0),

    -- Which external programme provided this mesocycle. The operator,
    -- 2026-09-06: "Squat 2x Int" is an external programme, provided by SBS, in
    -- the same way that "Peak Your Power Zones" is an external programme,
    -- provided by Peloton.
    provider           TEXT    CHECK (provider IS NULL
                                      OR length(trim(provider)) > 0),
    provided_programme TEXT    CHECK (provided_programme IS NULL
                                      OR length(trim(provided_programme)) > 0),
    -- 'v1' until 2026-08-18, when linear and block stopped being versions of one
    -- thing; 'test' from 2026-08-22, when a test stopped being a week of one.
    template           TEXT    NOT NULL
        CHECK (template IN ('linear', 'block', 'sbs', 'test')),

    -- The lift this mesocycle is about. For a test that is the lift being
    -- tested, which is the *next* mesocycle's primary rather than the
    -- predecessor's -- so it is stated by every template.
    primary_pattern    TEXT    NOT NULL
        CHECK (primary_pattern IN ('knee_dominant', 'hip_dominant',
                                   'upper_push', 'upper_pull')),
    primary_exercise   TEXT    NOT NULL,

    -- The starting 1RM, for a mesocycle that has one to start from.
    anchor_grams       INTEGER CHECK (anchor_grams IS NULL OR anchor_grams > 0),
    anchor_provenance  TEXT
        CHECK (anchor_provenance IS NULL
               OR anchor_provenance IN ('tested', 'estimated', 'asserted')),
    anchor_from        TEXT,
    -- Where the ladder opens, where the mesocycle states it rather than deriving
    -- it from the anchor above. Null derives it.
    opening_grams      INTEGER CHECK (opening_grams IS NULL OR opening_grams > 0),
    -- What the entry test failed above `anchor_grams`, if it found the ceiling.
    anchor_failed_grams INTEGER
        CHECK (anchor_failed_grams IS NULL OR anchor_failed_grams > anchor_grams),

    -- Which session's top set advances the plan. A test advances nothing: its
    -- own session is fixed at the heavy one and there is no ladder to gate.
    gating_role        TEXT    CHECK (gating_role IS NULL
                                        OR gating_role IN ('light', 'heavy')),
    start_date         TEXT    NOT NULL,
    duration_weeks     INTEGER NOT NULL,

    -- What a test is performed at, and what it is an attempt at.
    test_reps          INTEGER CHECK (test_reps IS NULL OR test_reps > 0),
    test_target_grams  INTEGER CHECK (test_target_grams IS NULL
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
    entry_test_reps    INTEGER CHECK (entry_test_reps IS NULL OR entry_test_reps > 0),
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

    -- A provider without a programme names nobody's programme.
    CHECK ((provider IS NULL) = (provided_programme IS NULL)),
    -- A provided progression must say what provided it. A derived one -- linear,
    -- or block periodisation -- was provided by nobody and may not claim to be.
    -- A test may be either: "Squat 2x Int Entry Test" is provided, and a week
    -- that only measures a lift is not.
    CHECK (template <> 'sbs' OR provider IS NOT NULL),
    CHECK (template IN ('sbs', 'test') OR provider IS NULL),

    -- One place in one plan.
    UNIQUE (plan, ordinal)
) STRICT;

-- Which microcycles of the external programme this mesocycle is.
--
-- **The published programme's own numbering, never ours.** An answer of
-- micros 1-2-4-5 keeps the four numbers that programme uses, so the third
-- microcycle here says 4 -- which is the way back to what was not chosen. Which
-- programme they are microcycles of is `gym_mesocycle.provided_programme`.
--
-- Ordered by `position`, and each week taken once: this is a selection from one
-- programme rather than a sequence of arbitrary weeks.
--
-- The cycling counterpart is `cycling_microcycle`, which arrived first and
-- carries its rides as well.
CREATE TABLE gym_microcycle (
    mesocycle  INTEGER NOT NULL REFERENCES gym_mesocycle(id) ON DELETE CASCADE,
    position   INTEGER NOT NULL CHECK (position >= 0),

    microcycle INTEGER NOT NULL CHECK (microcycle > 0),

    PRIMARY KEY (mesocycle, position),
    UNIQUE (mesocycle, microcycle)
) STRICT, WITHOUT ROWID;

CREATE TABLE gym_weekday (
    mesocycle  INTEGER NOT NULL REFERENCES gym_mesocycle(id) ON DELETE CASCADE,
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    role       TEXT    NOT NULL CHECK (role IN ('light', 'heavy')),

    PRIMARY KEY (mesocycle, weekday)
) STRICT, WITHOUT ROWID;

CREATE TABLE gym_interruption (
    mesocycle  INTEGER NOT NULL REFERENCES gym_mesocycle(id) ON DELETE CASCADE,

    start_date TEXT    NOT NULL,
    days       INTEGER NOT NULL CHECK (days >= 1 AND days <= 255),

    PRIMARY KEY (mesocycle, start_date)
) STRICT, WITHOUT ROWID;

CREATE TABLE gym_slot_fill (
    mesocycle  INTEGER NOT NULL REFERENCES gym_mesocycle(id) ON DELETE CASCADE,
    slot       TEXT    NOT NULL,
    role       TEXT    CHECK (role IS NULL OR role IN ('light', 'heavy')),

    position   INTEGER NOT NULL DEFAULT 0,
    exercise   TEXT    NOT NULL,

    static_sets INTEGER CHECK (static_sets IS NULL OR static_sets > 0),
    static_reps INTEGER CHECK (static_reps IS NULL OR static_reps > 0),
    CHECK ((static_sets IS NULL) = (static_reps IS NULL)),

    PRIMARY KEY (mesocycle, slot, role, position)
);

CREATE TABLE cycling_mesocycle (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,

    -- The plan this belongs to, and where in that plan's programme for this
    -- discipline it sits. **Identity is the plan's**: re-authoring a plan
    -- supersedes every mesocycle in it at once, which is what a per-mesocycle
    -- `name` used to do one level too low.
    plan               INTEGER NOT NULL REFERENCES plan(id) ON DELETE CASCADE,
    ordinal            INTEGER NOT NULL CHECK (ordinal > 0),

    -- Which external programme provided this mesocycle. Always present on this
    -- side: a cycling mesocycle is microcycles of a published Peloton
    -- programme, and there is no derived kind of one.
    provider           TEXT    NOT NULL CHECK (length(trim(provider)) > 0),
    provided_programme TEXT    NOT NULL
        CHECK (length(trim(provided_programme)) > 0),
    start_date         TEXT    NOT NULL,

    -- One place in one plan.
    UNIQUE (plan, ordinal)
) STRICT;

CREATE TABLE cycling_weekday (
    mesocycle  INTEGER NOT NULL REFERENCES cycling_mesocycle(id) ON DELETE CASCADE,
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    session    INTEGER NOT NULL CHECK (session > 0),

    PRIMARY KEY (mesocycle, weekday),
    UNIQUE (mesocycle, session)
) STRICT, WITHOUT ROWID;

CREATE TABLE cycling_microcycle (
    mesocycle          INTEGER NOT NULL REFERENCES cycling_mesocycle(id) ON DELETE CASCADE,
    -- This mesocycle's own order, from one.
    ordinal            INTEGER NOT NULL CHECK (ordinal > 0),

    -- The published programme's own numbering, never ours: an answer of
    -- micros 1-2-4-5 keeps the four numbers that programme uses, so the third
    -- microcycle here says 4. Which programme they are microcycles *of* is the
    -- mesocycle's to say, once, rather than repeated on every row here.
    published_ordinal  INTEGER NOT NULL CHECK (published_ordinal > 0),

    UNIQUE (mesocycle, published_ordinal),

    PRIMARY KEY (mesocycle, ordinal)
) STRICT, WITHOUT ROWID;

CREATE TABLE cycling_ride (
    mesocycle       INTEGER NOT NULL REFERENCES cycling_mesocycle(id) ON DELETE CASCADE,
    microcycle      INTEGER NOT NULL CHECK (microcycle > 0),
    -- This mesocycle's own order within the week, from one.
    session         INTEGER NOT NULL CHECK (session > 0),
    -- Which session of the published microcycle it was, in that mesocycle's own
    -- numbering. `cycling_microcycle.published_ordinal`'s counterpart, and the
    -- way back to the session the operator did not take.
    published_session INTEGER NOT NULL CHECK (published_session > 0),

    warm_up_seconds INTEGER NOT NULL CHECK (warm_up_seconds > 0),
    cool_down_seconds INTEGER CHECK (cool_down_seconds IS NULL OR cool_down_seconds > 0),
    effort_seconds  INTEGER CHECK (effort_seconds IS NULL OR effort_seconds > 0),

    PRIMARY KEY (mesocycle, microcycle, session),
    FOREIGN KEY (mesocycle, microcycle)
        REFERENCES cycling_microcycle(mesocycle, ordinal) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE cycling_venue (
    mesocycle   INTEGER NOT NULL REFERENCES cycling_mesocycle(id) ON DELETE CASCADE,
    microcycle  INTEGER NOT NULL,
    session     INTEGER NOT NULL,
    position    INTEGER NOT NULL CHECK (position >= 0),

    reference   TEXT    NOT NULL CHECK (length(trim(reference)) > 0),
    called      TEXT    NOT NULL CHECK (length(trim(called)) > 0),

    PRIMARY KEY (mesocycle, microcycle, session, position),
    FOREIGN KEY (mesocycle, microcycle, session)
        REFERENCES cycling_ride(mesocycle, microcycle, session) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE cycling_interval (
    mesocycle   INTEGER NOT NULL REFERENCES cycling_mesocycle(id) ON DELETE CASCADE,
    microcycle  INTEGER NOT NULL,
    session     INTEGER NOT NULL,
    position    INTEGER NOT NULL CHECK (position >= 0),

    zone        INTEGER NOT NULL CHECK (zone BETWEEN 1 AND 7),
    seconds     INTEGER NOT NULL CHECK (seconds > 0),

    PRIMARY KEY (mesocycle, microcycle, session, position),
    FOREIGN KEY (mesocycle, microcycle, session)
        REFERENCES cycling_ride(mesocycle, microcycle, session) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE prescribed_workout (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    mesocycle              INTEGER NOT NULL REFERENCES gym_mesocycle(id),

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

-- Unchanged in substance: a prescription that was performed is not deletable,
-- and neither is the delivery naming it (constitution 12.1).
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

-- The mesocycle that answers for a date is found by date, so the indexes are on
-- the date. The plan in force under a name is its latest authoring, which is a
-- `MAX` over the name.
CREATE INDEX gym_mesocycle_start ON gym_mesocycle(start_date);
CREATE INDEX cycling_mesocycle_start ON cycling_mesocycle(start_date);
CREATE INDEX plan_authored ON plan(name, authored_at);
