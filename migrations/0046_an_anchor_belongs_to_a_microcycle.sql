PRAGMA foreign_keys = OFF;

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

    gating_role        TEXT    CHECK (gating_role IS NULL
                                        OR gating_role IN ('light', 'heavy')),
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

    CHECK ((template = 'test') = (gating_role IS NULL)),
    CHECK ((template = 'test') = (test_reps IS NOT NULL)),

    CHECK (CASE template WHEN 'test' THEN duration_weeks = 1
                         WHEN 'sbs'  THEN duration_weeks = 4
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
    gating_role, start_date, duration_weeks,
    test_reps, entry_test_reps, entry_test_light_grams
)
SELECT
    id, plan, ordinal, provider, provided_programme, template,
    primary_pattern, primary_exercise,
    CASE WHEN template = 'test' THEN test_target_grams END,
    CASE WHEN template = 'test' AND test_target_grams IS NOT NULL
         THEN 'asserted' END,
    CASE WHEN template = 'test' AND test_target_grams IS NOT NULL
         THEN start_date END,
    NULL,
    gating_role, start_date, duration_weeks,
    test_reps, entry_test_reps, entry_test_light_grams
FROM gym_mesocycle;

DROP TABLE gym_mesocycle;
ALTER TABLE gym_mesocycle_rebuilt RENAME TO gym_mesocycle;

CREATE INDEX gym_mesocycle_start ON gym_mesocycle(start_date);

PRAGMA foreign_keys = ON;
