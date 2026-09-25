-- no-transaction
PRAGMA foreign_keys = OFF;

DROP TRIGGER prescribed_workout_performed_is_not_deletable;

CREATE TABLE prescribed_workout_rebuilt (
    id                     INTEGER PRIMARY KEY AUTOINCREMENT,
    mesocycle              INTEGER NOT NULL REFERENCES gym_mesocycle(id),

    issued_for             TEXT    NOT NULL,
    zone                   TEXT    NOT NULL,
    session_intensity      TEXT    NOT NULL CHECK (session_intensity IN ('higher', 'lower')),
    session_volume         TEXT    NOT NULL CHECK (session_volume IN ('higher', 'lower')),

    week_kind              TEXT    NOT NULL
        CHECK (week_kind IN ('climbing', 'test', 'holding')),
    week_index             INTEGER,

    anchor_grams           INTEGER CHECK (anchor_grams IS NULL OR anchor_grams > 0),
    anchor_provenance      TEXT
        CHECK (anchor_provenance IS NULL
               OR anchor_provenance IN ('tested', 'estimated', 'asserted')),
    anchor_from            TEXT,
    anchor_failed_grams    INTEGER
        CHECK (anchor_failed_grams IS NULL OR anchor_failed_grams > anchor_grams),

    target_grams           INTEGER CHECK (target_grams IS NULL OR target_grams > 0),

    parameters_authored_at TEXT    NOT NULL
        REFERENCES generation_parameters(authored_at),
    issued_at              TEXT    NOT NULL,

    UNIQUE (issued_for, issued_at),

    CHECK ((week_kind = 'climbing') = (week_index IS NOT NULL)),
    CHECK ((anchor_grams IS NULL) = (anchor_provenance IS NULL)),
    CHECK ((anchor_grams IS NULL) = (anchor_from IS NULL)),
    CHECK (week_kind = 'test' OR target_grams IS NULL),
    CHECK ((anchor_grams IS NULL) != (target_grams IS NULL))
) STRICT;

INSERT INTO prescribed_workout_rebuilt (id, mesocycle, issued_for, zone, session_intensity,
                                session_volume, week_kind, week_index, anchor_grams,
                                anchor_provenance, anchor_from, anchor_failed_grams,
                                target_grams, parameters_authored_at, issued_at)
    SELECT id, mesocycle, issued_for, zone, session_intensity, session_volume,
           week_kind, week_index, anchor_grams, anchor_provenance, anchor_from,
           anchor_failed_grams, target_grams, parameters_authored_at, issued_at
    FROM prescribed_workout;

DROP TABLE prescribed_workout;
ALTER TABLE prescribed_workout_rebuilt RENAME TO prescribed_workout;

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

DROP INDEX IF EXISTS gym_mesocycle_start;

CREATE TABLE gym_mesocycle_rebuilt (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,

    plan               INTEGER NOT NULL REFERENCES plan(id) ON DELETE CASCADE,
    ordinal            INTEGER NOT NULL CHECK (ordinal > 0),

    provider           TEXT    CHECK (provider IS NULL
                                      OR length(trim(provider)) > 0),
    provided_programme TEXT    CHECK (provided_programme IS NULL
                                      OR length(trim(provided_programme)) > 0),
    template           TEXT    NOT NULL
        CHECK (template IN ('linear', 'block', 'sbs', 'test')),

    primary_pattern    TEXT    NOT NULL
        CHECK (primary_pattern IN ('knee_dominant', 'hip_dominant',
                                   'upper_push', 'upper_pull')),
    primary_exercise   TEXT    NOT NULL,

    asserted_grams     INTEGER CHECK (asserted_grams IS NULL OR asserted_grams > 0),
    asserted_provenance TEXT
        CHECK (asserted_provenance IS NULL
               OR asserted_provenance IN ('tested', 'estimated', 'asserted')),
    asserted_from      TEXT,
    asserted_failed_grams INTEGER
        CHECK (asserted_failed_grams IS NULL
               OR asserted_failed_grams > asserted_grams),

    gating_intensity   TEXT    CHECK (gating_intensity IS NULL
                                        OR gating_intensity IN ('higher', 'lower')),
    gating_volume      TEXT    CHECK (gating_volume IS NULL
                                        OR gating_volume IN ('higher', 'lower')),
    start_date         TEXT    NOT NULL,
    duration_weeks     INTEGER NOT NULL,

    test_reps          INTEGER CHECK (test_reps IS NULL OR test_reps > 0),

    entry_test_reps    INTEGER CHECK (entry_test_reps IS NULL OR entry_test_reps > 0),
    entry_test_light_grams INTEGER CHECK (entry_test_light_grams IS NULL
                                          OR entry_test_light_grams > 0),

    CHECK (template = 'test' OR entry_test_reps IS NOT NULL OR asserted_grams IS NULL),
    CHECK ((asserted_grams IS NULL) = (asserted_provenance IS NULL)),
    CHECK ((asserted_grams IS NULL) = (asserted_from IS NULL)),
    CHECK (asserted_grams IS NOT NULL OR asserted_failed_grams IS NULL),

    CHECK ((gating_intensity IS NULL) = (gating_volume IS NULL)),
    CHECK ((template = 'test') = (gating_intensity IS NULL)),
    CHECK ((template = 'test') = (test_reps IS NOT NULL)),

    CHECK (CASE template WHEN 'test'   THEN duration_weeks = 1
                         WHEN 'sbs'    THEN duration_weeks = 4
                         WHEN 'linear' THEN duration_weeks >= 1
                         ELSE duration_weeks >= 2 END),

    CHECK (template = 'block' OR entry_test_reps IS NULL),
    CHECK (entry_test_reps IS NOT NULL OR entry_test_light_grams IS NULL),

    CHECK ((provider IS NULL) = (provided_programme IS NULL)),
    CHECK (template <> 'sbs' OR provider IS NOT NULL),
    CHECK (template IN ('sbs', 'test') OR provider IS NULL),

    UNIQUE (plan, ordinal)
) STRICT;

INSERT INTO gym_mesocycle_rebuilt (
    id, plan, ordinal, provider, provided_programme, template,
    primary_pattern, primary_exercise,
    asserted_grams, asserted_provenance, asserted_from, asserted_failed_grams,
    gating_intensity, gating_volume, start_date, duration_weeks,
    test_reps, entry_test_reps, entry_test_light_grams
)
SELECT
    id, plan, ordinal, provider, provided_programme, template,
    primary_pattern, primary_exercise,
    asserted_grams, asserted_provenance, asserted_from, asserted_failed_grams,
    gating_intensity, gating_volume, start_date, duration_weeks,
    test_reps, entry_test_reps, entry_test_light_grams
FROM gym_mesocycle;

DROP TABLE gym_mesocycle;
ALTER TABLE gym_mesocycle_rebuilt RENAME TO gym_mesocycle;

CREATE INDEX gym_mesocycle_start ON gym_mesocycle(start_date);

PRAGMA foreign_keys = ON;
