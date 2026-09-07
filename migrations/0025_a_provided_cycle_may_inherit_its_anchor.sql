-- A provided cycle may open from the one before it (issue #86).
--
-- **The anchor stops being required for `sbs`.** Every other progression states
-- the number its loads are shares of, and can: a ladder climbs from one and a
-- block's every load is a share of one, both knowable when the mesocycle is
-- authored. A provided cycle is the case where it is not. Week 4 day 2 is a
-- one-repetition maximum and is not optional, so a cycle that runs to its end
-- leaves a measured maximum behind -- and that maximum is what the next cycle
-- opens from, which is what makes the chart self-perpetuating (decision 0024).
--
-- So a plan holding three of them can only state the first. Authoring the autumn
-- on 7 September means saying what the cycle beginning 19 October opens from,
-- which is a test that has not happened; the alternatives were to assert a
-- number nobody has lifted, or to author the plan in pieces across three months.
-- The operator, 2026-09-07, agreeing the anchor should defer: *"the SBS
-- programme already does this for the rep maxes"* -- within a cycle each rep-max
-- day resets what the following week is a share of, and this is that same move
-- one level up.
--
-- **A null anchor is a statement, not a gap.** It claims nothing about a past
-- test; it defers to one. `Authoring` therefore has nothing to check for such a
-- row -- the rule that a `tested` anchor must point at a test that happened is
-- about a claim, and this makes none -- and prescribing resolves it against the
-- record, refusing where the predecessor measured nothing.
--
-- **A rebuild rather than an `ALTER`.** SQLite cannot drop a table `CHECK`, and
-- 0024 wrote one saying only a test may lack an anchor. Rows are carried: 0024
-- is already applied to the operator's store, and the authorisation to drop the
-- authored side was for that migration rather than a standing permission.

PRAGMA foreign_keys = OFF;

CREATE TABLE gym_mesocycle_rebuilt (
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

    -- A test has no anchor, no opening and no gate; a ladder and a block have an
    -- anchor and a gate, and may have an opening.
    --
    -- **A provided cycle may have no anchor, and that is inheritance** (issue
    -- #86, relaxed by 0025). It opens from whatever the mesocycle before it
    -- measured, which is not knowable when a plan holding three of them is
    -- authored in September. The absence is the statement: there is no number,
    -- and the record is asked for one when a session is prescribed.
    CHECK (CASE template WHEN 'test' THEN anchor_grams IS NULL
                         WHEN 'sbs'  THEN 1
                         ELSE anchor_grams IS NOT NULL END),
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

INSERT INTO gym_mesocycle_rebuilt
SELECT id, plan, ordinal, provider, provided_programme, template,
       primary_pattern, primary_exercise,
       anchor_grams, anchor_provenance, anchor_from, opening_grams,
       anchor_failed_grams, gating_role, start_date, duration_weeks,
       test_reps, test_target_grams, entry_test_reps, entry_test_light_grams
  FROM gym_mesocycle;

DROP TABLE gym_mesocycle;
ALTER TABLE gym_mesocycle_rebuilt RENAME TO gym_mesocycle;

-- Recreated with the table: an index goes with the table it is on.
CREATE INDEX gym_mesocycle_start ON gym_mesocycle(start_date);

PRAGMA foreign_keys = ON;
