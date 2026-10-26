-- no-transaction
PRAGMA foreign_keys = OFF;

CREATE TABLE programme (
    authored_at                TEXT    PRIMARY KEY,

    strength_reps_low          INTEGER NOT NULL CHECK (strength_reps_low > 0),
    strength_reps_high         INTEGER CHECK (strength_reps_high > strength_reps_low),
    strength_sets              INTEGER NOT NULL CHECK (strength_sets > 0),
    hypertrophy_reps_low       INTEGER NOT NULL CHECK (hypertrophy_reps_low > 0),
    hypertrophy_reps_high      INTEGER CHECK (hypertrophy_reps_high > hypertrophy_reps_low),
    hypertrophy_sets           INTEGER NOT NULL CHECK (hypertrophy_sets > 0),

    static_hold_seconds        INTEGER NOT NULL CHECK (static_hold_seconds > 0),

    rest_plyometric_low        INTEGER NOT NULL CHECK (rest_plyometric_low >= 0),
    rest_plyometric_high       INTEGER CHECK (rest_plyometric_high > rest_plyometric_low),
    rest_plyometric_ss_low     INTEGER CHECK (rest_plyometric_ss_low >= 0),
    rest_plyometric_ss_high    INTEGER CHECK (rest_plyometric_ss_high > rest_plyometric_ss_low),

    rest_power_low             INTEGER NOT NULL CHECK (rest_power_low >= 0),
    rest_power_high            INTEGER CHECK (rest_power_high > rest_power_low),
    rest_power_ss_low          INTEGER CHECK (rest_power_ss_low >= 0),
    rest_power_ss_high         INTEGER CHECK (rest_power_ss_high > rest_power_ss_low),

    rest_strength_low          INTEGER NOT NULL CHECK (rest_strength_low >= 0),
    rest_strength_high         INTEGER CHECK (rest_strength_high > rest_strength_low),
    rest_strength_ss_low       INTEGER CHECK (rest_strength_ss_low >= 0),
    rest_strength_ss_high      INTEGER CHECK (rest_strength_ss_high > rest_strength_ss_low),

    rest_hypertrophy_low       INTEGER NOT NULL CHECK (rest_hypertrophy_low >= 0),
    rest_hypertrophy_high      INTEGER CHECK (rest_hypertrophy_high > rest_hypertrophy_low),
    rest_hypertrophy_ss_low    INTEGER CHECK (rest_hypertrophy_ss_low >= 0),
    rest_hypertrophy_ss_high   INTEGER CHECK (rest_hypertrophy_ss_high > rest_hypertrophy_ss_low),

    rest_mobility_low          INTEGER NOT NULL CHECK (rest_mobility_low >= 0),
    rest_mobility_high         INTEGER CHECK (rest_mobility_high > rest_mobility_low),
    rest_mobility_ss_low       INTEGER CHECK (rest_mobility_ss_low >= 0),
    rest_mobility_ss_high      INTEGER CHECK (rest_mobility_ss_high > rest_mobility_ss_low)
) STRICT, WITHOUT ROWID;

CREATE TABLE programme_slot_scheme (
    authored_at TEXT    NOT NULL REFERENCES programme(authored_at),
    slot        TEXT    NOT NULL,
    reps_low    INTEGER NOT NULL CHECK (reps_low > 0),
    reps_high   INTEGER CHECK (reps_high > reps_low),
    sets        INTEGER NOT NULL CHECK (sets > 0),
    PRIMARY KEY (authored_at, slot)
) STRICT, WITHOUT ROWID;

CREATE TABLE linear_progression (
    authored_at                TEXT    PRIMARY KEY,

    light_of_heavy_bp          INTEGER NOT NULL CHECK (light_of_heavy_bp > 0),
    ladder_climb_grams         INTEGER NOT NULL CHECK (ladder_climb_grams > 0),
    entry_drop_bp              INTEGER NOT NULL CHECK (entry_drop_bp < 0),

    higher_top_set_reps        INTEGER NOT NULL CHECK (higher_top_set_reps > 0),
    lower_top_set_reps         INTEGER NOT NULL CHECK (lower_top_set_reps > 0),

    higher_back_off_sets       INTEGER NOT NULL CHECK (higher_back_off_sets > 0),
    higher_back_off_reps       INTEGER NOT NULL CHECK (higher_back_off_reps > 0),
    higher_back_off_bp         INTEGER NOT NULL CHECK (higher_back_off_bp > 0),
    lower_back_off_sets        INTEGER NOT NULL CHECK (lower_back_off_sets > 0),
    lower_back_off_reps        INTEGER NOT NULL CHECK (lower_back_off_reps > 0),
    lower_back_off_bp          INTEGER NOT NULL CHECK (lower_back_off_bp > 0),

    first_reset_drop_bp        INTEGER NOT NULL CHECK (first_reset_drop_bp < 0),
    first_reset_reclimb_grams  INTEGER NOT NULL CHECK (first_reset_reclimb_grams > 0),
    second_reset_drop_bp       INTEGER NOT NULL CHECK (second_reset_drop_bp < 0),
    second_reset_reclimb_grams INTEGER NOT NULL CHECK (second_reset_reclimb_grams > 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE linear_warmup_step (
    authored_at   TEXT    NOT NULL REFERENCES linear_progression(authored_at),
    position      INTEGER NOT NULL CHECK (position >= 0),
    of_top_set_bp INTEGER NOT NULL CHECK (of_top_set_bp > 0),
    reps          INTEGER NOT NULL CHECK (reps > 0),
    PRIMARY KEY (authored_at, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE sbs_progression (
    authored_at TEXT PRIMARY KEY
) STRICT, WITHOUT ROWID;

CREATE TABLE sbs_warmup_step (
    authored_at   TEXT    NOT NULL REFERENCES sbs_progression(authored_at),
    position      INTEGER NOT NULL CHECK (position >= 0),
    of_top_set_bp INTEGER NOT NULL CHECK (of_top_set_bp > 0),
    reps          INTEGER NOT NULL CHECK (reps > 0),
    PRIMARY KEY (authored_at, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE sbs_chart_day (
    authored_at        TEXT    NOT NULL REFERENCES sbs_progression(authored_at),
    week               INTEGER NOT NULL CHECK (week BETWEEN 1 AND 4),
    session            TEXT    NOT NULL CHECK (session IN ('first', 'second')),
    kind               TEXT    NOT NULL CHECK (kind IN ('percentage', 'rep_max', 'test')),

    reps               INTEGER NOT NULL CHECK (reps > 0),
    sets               INTEGER CHECK (sets > 0),
    share_bp           INTEGER CHECK (share_bp > 0),
    back_off_sets      INTEGER CHECK (back_off_sets > 0),
    back_off_reps_low  INTEGER CHECK (back_off_reps_low > 0),
    back_off_reps_high INTEGER CHECK (back_off_reps_high > back_off_reps_low),

    PRIMARY KEY (authored_at, week, session),

    CHECK ((kind = 'percentage') = (sets IS NOT NULL)),
    CHECK ((kind = 'percentage') = (share_bp IS NOT NULL)),
    CHECK ((kind = 'rep_max') = (back_off_sets IS NOT NULL)),
    CHECK ((kind = 'rep_max') = (back_off_reps_low IS NOT NULL)),
    CHECK (kind = 'rep_max' OR back_off_reps_high IS NULL)
) STRICT, WITHOUT ROWID;

INSERT INTO programme (
    authored_at,
    strength_reps_low, strength_reps_high, strength_sets,
    hypertrophy_reps_low, hypertrophy_reps_high, hypertrophy_sets,
    static_hold_seconds,
    rest_plyometric_low, rest_plyometric_high, rest_plyometric_ss_low, rest_plyometric_ss_high,
    rest_power_low, rest_power_high, rest_power_ss_low, rest_power_ss_high,
    rest_strength_low, rest_strength_high, rest_strength_ss_low, rest_strength_ss_high,
    rest_hypertrophy_low, rest_hypertrophy_high, rest_hypertrophy_ss_low, rest_hypertrophy_ss_high,
    rest_mobility_low, rest_mobility_high, rest_mobility_ss_low, rest_mobility_ss_high
)
SELECT authored_at,
       4, 6, 3,
       4, 6, 3,
       static_hold_seconds,
       rest_plyometric_low, rest_plyometric_high, rest_plyometric_ss_low, rest_plyometric_ss_high,
       rest_power_low, rest_power_high, rest_power_ss_low, rest_power_ss_high,
       rest_strength_low, rest_strength_high, rest_strength_ss_low, rest_strength_ss_high,
       rest_hypertrophy_low, rest_hypertrophy_high, rest_hypertrophy_ss_low, rest_hypertrophy_ss_high,
       rest_mobility_low, rest_mobility_high, rest_mobility_ss_low, rest_mobility_ss_high
FROM generation_parameters;

INSERT INTO programme_slot_scheme (authored_at, slot, reps_low, reps_high, sets)
SELECT g.authored_at, s.slot, 8, 12, 3
FROM generation_parameters AS g
CROSS JOIN (SELECT 'wrist_flexion' AS slot UNION ALL SELECT 'wrist_extension') AS s;

INSERT INTO linear_progression (
    authored_at, light_of_heavy_bp, ladder_climb_grams, entry_drop_bp,
    higher_top_set_reps, lower_top_set_reps,
    higher_back_off_sets, higher_back_off_reps, higher_back_off_bp,
    lower_back_off_sets, lower_back_off_reps, lower_back_off_bp,
    first_reset_drop_bp, first_reset_reclimb_grams,
    second_reset_drop_bp, second_reset_reclimb_grams
)
SELECT authored_at, light_of_heavy_bp, ladder_climb_grams, entry_drop_bp,
       1, 3,
       2, 4, 8500,
       3, 6, 8500,
       -1000, 5000,
       -500, 2500
FROM generation_parameters;

INSERT INTO linear_warmup_step (authored_at, position, of_top_set_bp, reps)
SELECT g.authored_at, r.position, r.of_top_set_bp, r.reps
FROM generation_parameters AS g
CROSS JOIN (
    SELECT 0 AS position, 4000 AS of_top_set_bp, 4 AS reps
    UNION ALL SELECT 1, 6000, 3
    UNION ALL SELECT 2, 8000, 2
    UNION ALL SELECT 3, 9000, 1
) AS r;

INSERT INTO sbs_progression (authored_at)
SELECT authored_at FROM generation_parameters;

INSERT INTO sbs_warmup_step (authored_at, position, of_top_set_bp, reps)
SELECT g.authored_at, r.position, r.of_top_set_bp, r.reps
FROM generation_parameters AS g
CROSS JOIN (
    SELECT 0 AS position, 4000 AS of_top_set_bp, 4 AS reps
    UNION ALL SELECT 1, 6000, 3
    UNION ALL SELECT 2, 8000, 2
    UNION ALL SELECT 3, 9000, 1
) AS r;

INSERT INTO sbs_chart_day (
    authored_at, week, session, kind, reps, sets, share_bp,
    back_off_sets, back_off_reps_low, back_off_reps_high
)
SELECT g.authored_at, d.week, d.session, d.kind, d.reps, d.sets, d.share_bp,
       d.back_off_sets, d.back_off_reps_low, d.back_off_reps_high
FROM generation_parameters AS g
CROSS JOIN (
    SELECT 1 AS week, 'first' AS session, 'percentage' AS kind, 5 AS reps, 5 AS sets,
           8000 AS share_bp, NULL AS back_off_sets, NULL AS back_off_reps_low,
           NULL AS back_off_reps_high
    UNION ALL SELECT 1, 'second', 'rep_max', 8, NULL, NULL, 3, 5, 6
    UNION ALL SELECT 2, 'first', 'percentage', 3, 4, 8500, NULL, NULL, NULL
    UNION ALL SELECT 2, 'second', 'rep_max', 5, NULL, NULL, 3, 3, 4
    UNION ALL SELECT 3, 'first', 'percentage', 1, 3, 9000, NULL, NULL, NULL
    UNION ALL SELECT 3, 'second', 'rep_max', 3, NULL, NULL, 3, 1, 2
    UNION ALL SELECT 4, 'first', 'percentage', 3, 3, 7500, NULL, NULL, NULL
    UNION ALL SELECT 4, 'second', 'test', 1, NULL, NULL, NULL, NULL, NULL
) AS d;

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
                                target_grams, issued_at)
    SELECT id, mesocycle, issued_for, zone, session_intensity, session_volume,
           week_kind, week_index, anchor_grams, anchor_provenance, anchor_from,
           anchor_failed_grams, target_grams, issued_at
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

DROP TABLE generation_load_scale;
DROP TABLE generation_parameters;

PRAGMA foreign_keys = ON;
