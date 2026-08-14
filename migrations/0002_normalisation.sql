-- The normalised layer for Hevy workouts: a derivation, not an input.
--
-- The difference from `0001` is visible in what is absent. Raw carries
-- append-only triggers because it is an input and losing one is losing a fact.
-- These tables carry none: § II says a derivation is never mutated in place and
-- must be identical to a full re-derivation, and the cheapest way to be sure of
-- that is to do the full re-derivation every time. A trigger forbidding deletes
-- here would forbid exactly the rebuild the constitution requires.
--
-- Everything below hangs off `landing_record_id`, and that is the whole of how
-- supersession stays unresolved (§ 10). Two records for one workout are two
-- rows here, both standing, neither marked current. Keying on the source's
-- identifier would have collapsed the pair silently.

CREATE TABLE normalisation_run (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    stream               TEXT    NOT NULL,
    started_at           TEXT    NOT NULL,
    finished_at          TEXT,
    outcome              TEXT    CHECK (outcome IN ('succeeded', 'failed')),
    records_read         INTEGER,
    workouts_written     INTEGER,
    workouts_withdrawn   INTEGER,
    retractions_applied  INTEGER,
    records_refused      INTEGER,
    refusals_recorded    INTEGER,
    failure_reason       TEXT,

    -- These mirror `NormalisationOutcome`. The type makes the invalid
    -- combinations unrepresentable in Rust; these make them unrepresentable in
    -- the file, including to a writer that is not this program.
    CHECK ((outcome IS NULL) = (finished_at IS NULL)),
    -- A finished derivation reports every count, because the point of them is
    -- that they add up: a record became a workout, a withdrawn workout, a
    -- retraction, or a refusal, and no record has two outcomes (FR-005).
    CHECK (outcome IS NULL OR (
        records_read IS NOT NULL AND workouts_written IS NOT NULL
        AND workouts_withdrawn IS NOT NULL AND retractions_applied IS NOT NULL
        AND records_refused IS NOT NULL AND refusals_recorded IS NOT NULL
    )),
    -- A failure always says why; a success never does.
    CHECK ((outcome = 'failed') = (failure_reason IS NOT NULL))
);

CREATE INDEX normalisation_run_succeeded
    ON normalisation_run (stream, finished_at DESC)
    WHERE outcome = 'succeeded';

CREATE TABLE gym_workout (
    -- Not a surrogate key. One workout per landing record is what § II.3 means
    -- by per-record, and it makes a re-derivation an exact replacement.
    landing_record_id  INTEGER PRIMARY KEY REFERENCES hevy_workout_landing(id),
    source_record_id   TEXT    NOT NULL,
    -- Instant plus zone, never an offset. Given the zone the two forms are
    -- losslessly interconvertible, and an offset records the rule that applied
    -- at one instant rather than the rule that applies across an interval.
    started_at_utc     TEXT    NOT NULL,
    zone               TEXT    NOT NULL,
    -- Provenance, mandatory (§ II.3). What the source told us, not inferred.
    endpoint           TEXT    NOT NULL,
    event_kind         TEXT    NOT NULL,
    event_time         TEXT,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id)
);

CREATE INDEX gym_workout_by_source_record
    ON gym_workout (source_record_id);

-- One position in a workout's ordered sequence.
CREATE TABLE workout_item (
    workout       INTEGER NOT NULL REFERENCES gym_workout(landing_record_id),
    position      INTEGER NOT NULL,
    is_superset   INTEGER NOT NULL CHECK (is_superset IN (0, 1)),

    PRIMARY KEY (workout, position)
) WITHOUT ROWID;

CREATE TABLE performed_exercise (
    workout        INTEGER NOT NULL,
    item_position  INTEGER NOT NULL,
    -- Where in the item. Zero for a plain exercise; a superset's members count
    -- from zero in the order they were performed.
    position       INTEGER NOT NULL,
    exercise       TEXT    NOT NULL,
    measure        TEXT    NOT NULL
        CHECK (measure IN ('reps', 'duration', 'distance', 'timed-distance')),

    PRIMARY KEY (workout, item_position, position),
    FOREIGN KEY (workout, item_position) REFERENCES workout_item(workout, position)
) WITHOUT ROWID;

CREATE TABLE performed_set (
    workout            INTEGER NOT NULL,
    item_position      INTEGER NOT NULL,
    exercise_position  INTEGER NOT NULL,
    position           INTEGER NOT NULL,

    -- `Load`, written flat. Absolute cannot be zero, which is the invariant the
    -- whole load model turns on, so the file says so too.
    load_kind          TEXT    NOT NULL CHECK (load_kind IN ('absolute', 'relative')),
    load_grams         INTEGER NOT NULL,

    -- The measure, written flat. Exactly the column the exercise's measure
    -- names is populated — the sum type projected, not a guess to be read back
    -- as "whichever column is filled". The translator never reads it that way:
    -- the exercise says which measure applies and the column follows.
    reps               INTEGER,
    duration_seconds   INTEGER,
    distance_mm        INTEGER,

    -- Absent is absent: not zero, and not carried forward from a neighbour.
    rir                TEXT,
    set_kind           TEXT    NOT NULL CHECK (set_kind IN ('working', 'warmup')),
    -- This source records none and none is invented (§ 11, § 37). The column
    -- exists because rest is a fact about a set even where nothing records it.
    rest_after_seconds INTEGER,

    PRIMARY KEY (workout, item_position, exercise_position, position),
    FOREIGN KEY (workout, item_position, exercise_position)
        REFERENCES performed_exercise(workout, item_position, position),

    -- Zero is impossible on the absolute axis and meaningful on the relative
    -- one, where it is plain bodyweight.
    CHECK (load_kind = 'relative' OR load_grams > 0),
    -- Which columns the measure populates cannot be checked here — the measure
    -- is on `performed_exercise` and a CHECK sees one row. These two hold
    -- whatever it is: a set is counted in something, and repetitions are not
    -- ground covered.
    CHECK ((reps IS NOT NULL) + (duration_seconds IS NOT NULL)
           + (distance_mm IS NOT NULL) >= 1),
    CHECK (reps IS NULL OR (duration_seconds IS NULL AND distance_mm IS NULL)),
    -- A rep count of zero is an attempt, not a set.
    CHECK (reps IS NULL OR reps > 0)
) WITHOUT ROWID;

-- What the domain would not accept, queryable after a derivation (FR-023).
--
-- The reason is a key rather than a sentence, so "the refusals are exactly the
-- named set" is a `WHERE` clause and not a grep. `kind` is derived from it and
-- stored anyway, because grouping by what an operator should *do* is the
-- question this table is read to answer.
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

    -- Which of ours it belonged to, where that was known by the time it was
    -- refused. A position alone is not enough to act on.
    exercise           TEXT,

    reason             TEXT    NOT NULL,
    kind               TEXT    NOT NULL
        CHECK (kind IN ('wrong data', 'declared limitation', 'unmodelled')),
    detail             TEXT,

    -- The locus decides which position columns mean anything.
    CHECK (locus_kind != 'record'
           OR (entry_index IS NULL AND set_index IS NULL AND group_id IS NULL)),
    CHECK (locus_kind != 'entry'
           OR (entry_index IS NOT NULL AND set_index IS NULL AND group_id IS NULL)),
    CHECK (locus_kind != 'set'
           OR (entry_index IS NOT NULL AND set_index IS NOT NULL AND group_id IS NULL)),
    CHECK (locus_kind != 'grouping'
           OR (group_id IS NOT NULL AND entry_index IS NULL AND set_index IS NULL))
);

CREATE INDEX normalisation_refusal_by_kind
    ON normalisation_refusal (kind, reason);
