-- A work-up can state what it expects.
--
-- `Prescribed::Autoregulated` pins no load, which is right: the whole point of
-- working up is that the day decides. But "the plan pins no load" and "the plan
-- has no expectation" are different facts, and the row could only hold the
-- first.
--
-- **Two things went wrong because of it.** SBS's repetition-maximum day derives
-- its target from the maximum current that week -- `sbs_load` computes it,
-- `PrimaryLoad::RepMax` carries it, and the ramp is built as a share of it --
-- and then the set threw it away, so the operator was told to work up to
-- nothing. And the back-off sets, which the chart states as `3 x 5-6 @ 8RM`,
-- were written as autoregulated too, which said they were taken to failure. They
-- are not. They are ordinary sets at the load the top set found.
--
-- The back-offs need no schema: they are `fixed` now, like any other set at a
-- stated load. This column is for the top set alone.
--
-- **Why not reuse `load_grams`.** An autoregulated set is forbidden a load by a
-- `CHECK`, and that `CHECK` is worth keeping -- it is what stops a work-up being
-- silently turned into a fixed set by a careless write. A target is a different
-- column because it is a different claim: `load_grams` is what you must lift,
-- `toward_grams` is what the plan thinks you will.
--
-- **A block's exit test keeps `NULL`.** Decision 0011 makes its target a
-- function of where the preceding progression stands, so it moves as the record
-- does and a number stored at issue would be stale the first time a session goes
-- up. Only a target the plan can state for the session it is issuing belongs
-- here, which today means SBS's.
--
-- One table and no graph: nothing references `prescribed_set`.

ALTER TABLE prescribed_set RENAME TO prescribed_set_old;

CREATE TABLE prescribed_set (
    workout            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    exercise_position  INTEGER NOT NULL,
    position           INTEGER NOT NULL,

    variant            TEXT    NOT NULL
        CHECK (variant IN ('fixed', 'to_effort', 'autoregulated')),

    load_kind          TEXT    CHECK (load_kind IN ('absolute', 'relative')),
    load_grams         INTEGER,

    -- What a work-up expects to reach. Never an instruction, and never a cap.
    toward_kind        TEXT    CHECK (toward_kind IN ('absolute', 'relative')),
    toward_grams       INTEGER,

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

    -- Only a work-up has something to expect. Every other variant pins a load,
    -- and a second number beside it would be two prescriptions in one row.
    CHECK (variant = 'autoregulated' OR toward_kind IS NULL),

    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK ((toward_kind IS NULL) = (toward_grams IS NULL)),
    CHECK ((target_kind IS NULL) = (target_low IS NULL)),
    CHECK (target_high IS NULL OR target_low IS NOT NULL),

    CHECK (target_high IS NULL OR target_high > target_low),
    CHECK ((rest_high_seconds IS NULL) OR (rest_low_seconds IS NOT NULL)),
    CHECK (rest_high_seconds IS NULL OR rest_high_seconds > rest_low_seconds)
) STRICT, WITHOUT ROWID;

INSERT INTO prescribed_set (
    workout, item_position, exercise_position, position,
    variant, load_kind, load_grams,
    target_kind, target_low, target_high,
    effort, rest_low_seconds, rest_high_seconds, warmup
)
    SELECT workout, item_position, exercise_position, position,
           variant, load_kind, load_grams,
           target_kind, target_low, target_high,
           effort, rest_low_seconds, rest_high_seconds, warmup
      FROM prescribed_set_old;

DROP TABLE prescribed_set_old;
