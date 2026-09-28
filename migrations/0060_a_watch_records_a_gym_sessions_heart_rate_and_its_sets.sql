CREATE TABLE measured_gym_session (
    workout                 INTEGER PRIMARY KEY REFERENCES gym_workout(id),
    duration_seconds        INTEGER NOT NULL,
    average_bpm             INTEGER,
    highest_bpm             INTEGER,
    sets_landing_record_id  INTEGER,
    CHECK (duration_seconds >= 0),
    CHECK ((average_bpm IS NULL) = (highest_bpm IS NULL)),
    CHECK (average_bpm IS NULL OR average_bpm > 0),
    CHECK (highest_bpm IS NULL OR highest_bpm > 0)
) STRICT;

CREATE TABLE measured_set (
    workout     INTEGER NOT NULL REFERENCES gym_workout(id),
    position    INTEGER NOT NULL,
    started_at_utc TEXT NOT NULL,
    zone        TEXT    NOT NULL,
    reps        INTEGER NOT NULL,
    load_kind   TEXT    CHECK (load_kind IN ('absolute', 'relative')),
    load_grams  INTEGER,
    guess       TEXT,
    guess_from  TEXT    CHECK (guess_from IN ('proposed', 'from-its-run')),
    PRIMARY KEY (workout, position),
    CHECK (reps > 0),
    CHECK ((guess IS NULL) = (guess_from IS NULL)),
    CHECK ((load_kind IS NULL) = (load_grams IS NULL)),
    CHECK (load_kind IS NOT 'absolute' OR load_grams >= 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE normalisation_refusal_next (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id),
    stream             TEXT    NOT NULL,
    landing_record_id  INTEGER NOT NULL,
    source_record_id   TEXT    NOT NULL,

    locus_kind         TEXT    NOT NULL
        CHECK (locus_kind IN ('record', 'entry', 'set', 'grouping', 'ungrouped')),
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
           OR (group_id IS NOT NULL AND entry_index IS NULL AND set_index IS NULL)),
    CHECK (locus_kind != 'ungrouped'
           OR (set_index IS NOT NULL AND entry_index IS NULL AND group_id IS NULL))
) STRICT;

INSERT INTO normalisation_refusal_next (
    id, run_id, stream, landing_record_id, source_record_id,
    locus_kind, entry_index, set_index, group_id, exercise, reason, kind, detail
)
SELECT id, run_id, stream, landing_record_id, source_record_id,
       locus_kind, entry_index, set_index, group_id, exercise, reason, kind, detail
FROM normalisation_refusal;

DROP TABLE normalisation_refusal;

ALTER TABLE normalisation_refusal_next RENAME TO normalisation_refusal;

CREATE INDEX normalisation_refusal_by_kind
    ON normalisation_refusal (kind, reason);

CREATE INDEX normalisation_refusal_by_stream
    ON normalisation_refusal (stream, id);
