-- A failed attempt becomes an outcome rather than a refusal.
--
-- `0002` refused a zero-rep set as unmodelled, and said why in its own CHECK:
-- "a rep count of zero is an attempt, not a set". That was right about zero not
-- being a count and wrong about where the case goes. The domain can express a
-- failed attempt now that the prescribed side exists to give it meaning, and the
-- negative gate in `docs/primary-lift-progression.md` depends on it — a stall is
-- detected from a failure, so a failure the layer will not represent is a stall
-- the programme cannot see.
--
-- The discriminator is the rep count and not the source's `failure` set type.
-- That type means "taken to failure" and sits on 77 completed sets in the
-- corpus against one genuine failure; keying on it would misfile 76.
--
-- SQLite cannot add or relax a CHECK in place, so this is the documented
-- twelve-step rebuild. That is cheap here for the reason `0002` gives: these
-- tables hold a derivation, never an input, so the worst a failed copy costs is
-- a re-derivation rather than a fact.

PRAGMA foreign_keys = OFF;

CREATE TABLE performed_set_new (
    workout            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    exercise_position  INTEGER NOT NULL,
    position           INTEGER NOT NULL,

    load_kind          TEXT    NOT NULL CHECK (load_kind IN ('absolute', 'relative')),
    load_grams         INTEGER NOT NULL,

    -- What became of the set. `Performed<M>` projected: a completed set carries
    -- its measure, a failed attempt carries none, and there is no third state.
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

    -- A failed attempt is a load and nothing else. The measure it *would* have
    -- been is not lost: `performed_exercise.measure` carries it, as it already
    -- does for every set of that entry.
    CHECK (outcome != 'failed'
           OR (reps IS NULL AND duration_seconds IS NULL AND distance_mm IS NULL)),

    -- A completed set is counted in something, and repetitions are not ground
    -- covered. Both were unconditional in `0002` and are now conditional on the
    -- outcome, which is the whole of what this migration relaxes.
    CHECK (outcome != 'completed'
           OR (reps IS NOT NULL) + (duration_seconds IS NOT NULL)
              + (distance_mm IS NOT NULL) >= 1),
    CHECK (reps IS NULL OR (duration_seconds IS NULL AND distance_mm IS NULL)),

    -- Unchanged, and still the reason `RepCount` is a `NonZeroU32`. Zero is not
    -- a small number of repetitions; it is a different outcome.
    CHECK (reps IS NULL OR reps > 0)
) WITHOUT ROWID;

-- Every existing row is a completed set by construction: `0002` would not
-- accept anything else.
INSERT INTO performed_set_new (
    workout, item_position, exercise_position, position,
    load_kind, load_grams, outcome,
    reps, duration_seconds, distance_mm,
    rir, set_kind, rest_after_seconds
)
SELECT
    workout, item_position, exercise_position, position,
    load_kind, load_grams, 'completed',
    reps, duration_seconds, distance_mm,
    rir, set_kind, rest_after_seconds
FROM performed_set;

DROP TABLE performed_set;

ALTER TABLE performed_set_new RENAME TO performed_set;

PRAGMA foreign_keys = ON;
