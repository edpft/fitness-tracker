-- no-transaction
PRAGMA foreign_keys = OFF;

CREATE TABLE workout (
    intensity           TEXT    NOT NULL CHECK (intensity IN ('higher', 'lower')),
    authored_at         TEXT    NOT NULL,

    reps_low            INTEGER NOT NULL CHECK (reps_low > 0),
    reps_high           INTEGER CHECK (reps_high > reps_low),
    sets                INTEGER NOT NULL CHECK (sets > 0),

    rest_low            INTEGER NOT NULL CHECK (rest_low >= 0),
    rest_high           INTEGER CHECK (rest_high > rest_low),
    rest_ss_low         INTEGER CHECK (rest_ss_low >= 0),
    rest_ss_high        INTEGER CHECK (rest_ss_high > rest_ss_low),

    static_hold_seconds INTEGER NOT NULL CHECK (static_hold_seconds > 0),

    PRIMARY KEY (intensity, authored_at)
) STRICT, WITHOUT ROWID;

CREATE TABLE workout_reps (
    intensity   TEXT    NOT NULL,
    authored_at TEXT    NOT NULL,
    level       TEXT    NOT NULL CHECK (level IN ('section', 'superset', 'exercise')),
    name        TEXT    NOT NULL,
    reps_low    INTEGER NOT NULL CHECK (reps_low > 0),
    reps_high   INTEGER CHECK (reps_high > reps_low),
    sets        INTEGER NOT NULL CHECK (sets > 0),
    PRIMARY KEY (intensity, authored_at, level, name),
    FOREIGN KEY (intensity, authored_at) REFERENCES workout(intensity, authored_at)
) STRICT, WITHOUT ROWID;

CREATE TABLE workout_rest (
    intensity    TEXT    NOT NULL,
    authored_at  TEXT    NOT NULL,
    level        TEXT    NOT NULL CHECK (level IN ('section', 'superset', 'exercise')),
    name         TEXT    NOT NULL,
    rest_low     INTEGER NOT NULL CHECK (rest_low >= 0),
    rest_high    INTEGER CHECK (rest_high > rest_low),
    rest_ss_low  INTEGER CHECK (rest_ss_low >= 0),
    rest_ss_high INTEGER CHECK (rest_ss_high > rest_ss_low),
    PRIMARY KEY (intensity, authored_at, level, name),
    FOREIGN KEY (intensity, authored_at) REFERENCES workout(intensity, authored_at),
    CHECK (level = 'section' OR rest_ss_low IS NULL)
) STRICT, WITHOUT ROWID;

CREATE TABLE linear_rest (
    authored_at   TEXT    PRIMARY KEY REFERENCES linear_progression(authored_at),
    top_set_low   INTEGER NOT NULL CHECK (top_set_low >= 0),
    top_set_high  INTEGER CHECK (top_set_high > top_set_low),
    back_off_low  INTEGER NOT NULL CHECK (back_off_low >= 0),
    back_off_high INTEGER CHECK (back_off_high > back_off_low),
    attempt_low   INTEGER NOT NULL CHECK (attempt_low >= 0),
    attempt_high  INTEGER CHECK (attempt_high > attempt_low)
) STRICT, WITHOUT ROWID;

CREATE TABLE sbs_rest (
    authored_at   TEXT    PRIMARY KEY REFERENCES sbs_progression(authored_at),
    across_low    INTEGER NOT NULL CHECK (across_low >= 0),
    across_high   INTEGER CHECK (across_high > across_low),
    rep_max_low   INTEGER NOT NULL CHECK (rep_max_low >= 0),
    rep_max_high  INTEGER CHECK (rep_max_high > rep_max_low),
    back_off_low  INTEGER NOT NULL CHECK (back_off_low >= 0),
    back_off_high INTEGER CHECK (back_off_high > back_off_low),
    attempt_low   INTEGER NOT NULL CHECK (attempt_low >= 0),
    attempt_high  INTEGER CHECK (attempt_high > attempt_low)
) STRICT, WITHOUT ROWID;

INSERT INTO workout (
    intensity, authored_at, reps_low, reps_high, sets,
    rest_low, rest_high, rest_ss_low, rest_ss_high, static_hold_seconds
)
SELECT i.intensity, p.authored_at,
       p.strength_reps_low, p.strength_reps_high, p.strength_sets,
       p.rest_strength_low, p.rest_strength_high, p.rest_strength_ss_low, p.rest_strength_ss_high,
       p.static_hold_seconds
FROM programme AS p
CROSS JOIN (SELECT 'higher' AS intensity UNION ALL SELECT 'lower') AS i;

INSERT INTO workout_reps (intensity, authored_at, level, name, reps_low, reps_high, sets)
SELECT i.intensity, p.authored_at, 'section', 'strength',
       p.strength_reps_low, p.strength_reps_high, p.strength_sets
FROM programme AS p
CROSS JOIN (SELECT 'higher' AS intensity UNION ALL SELECT 'lower') AS i
UNION ALL
SELECT i.intensity, p.authored_at, 'section', 'hypertrophy',
       p.hypertrophy_reps_low, p.hypertrophy_reps_high, p.hypertrophy_sets
FROM programme AS p
CROSS JOIN (SELECT 'higher' AS intensity UNION ALL SELECT 'lower') AS i
UNION ALL
SELECT i.intensity, s.authored_at, 'exercise', s.slot, s.reps_low, s.reps_high, s.sets
FROM programme_slot_scheme AS s
CROSS JOIN (SELECT 'higher' AS intensity UNION ALL SELECT 'lower') AS i;

INSERT INTO workout_rest (
    intensity, authored_at, level, name, rest_low, rest_high, rest_ss_low, rest_ss_high
)
SELECT i.intensity, p.authored_at, 'section', 'plyometric',
       p.rest_plyometric_low, p.rest_plyometric_high, p.rest_plyometric_ss_low, p.rest_plyometric_ss_high
FROM programme AS p
CROSS JOIN (SELECT 'higher' AS intensity UNION ALL SELECT 'lower') AS i
UNION ALL
SELECT i.intensity, p.authored_at, 'section', 'power',
       p.rest_power_low, p.rest_power_high, p.rest_power_ss_low, p.rest_power_ss_high
FROM programme AS p
CROSS JOIN (SELECT 'higher' AS intensity UNION ALL SELECT 'lower') AS i
UNION ALL
SELECT i.intensity, p.authored_at, 'section', 'strength',
       p.rest_strength_low, p.rest_strength_high, p.rest_strength_ss_low, p.rest_strength_ss_high
FROM programme AS p
CROSS JOIN (SELECT 'higher' AS intensity UNION ALL SELECT 'lower') AS i
UNION ALL
SELECT i.intensity, p.authored_at, 'section', 'hypertrophy',
       p.rest_hypertrophy_low, p.rest_hypertrophy_high, p.rest_hypertrophy_ss_low, p.rest_hypertrophy_ss_high
FROM programme AS p
CROSS JOIN (SELECT 'higher' AS intensity UNION ALL SELECT 'lower') AS i
UNION ALL
SELECT i.intensity, p.authored_at, 'section', 'mobility',
       p.rest_mobility_low, p.rest_mobility_high, p.rest_mobility_ss_low, p.rest_mobility_ss_high
FROM programme AS p
CROSS JOIN (SELECT 'higher' AS intensity UNION ALL SELECT 'lower') AS i;

INSERT INTO linear_rest (
    authored_at, top_set_low, top_set_high, back_off_low, back_off_high, attempt_low, attempt_high
)
SELECT l.authored_at,
       p.rest_strength_low, p.rest_strength_high,
       p.rest_strength_low, p.rest_strength_high,
       p.rest_strength_low, p.rest_strength_high
FROM linear_progression AS l
JOIN programme AS p ON p.authored_at = COALESCE(
    (SELECT q.authored_at FROM programme AS q WHERE q.authored_at = l.authored_at),
    (SELECT q.authored_at FROM programme AS q ORDER BY q.authored_at DESC LIMIT 1)
);

INSERT INTO sbs_rest (
    authored_at, across_low, across_high, rep_max_low, rep_max_high,
    back_off_low, back_off_high, attempt_low, attempt_high
)
SELECT b.authored_at,
       p.rest_strength_low, p.rest_strength_high,
       p.rest_strength_low, p.rest_strength_high,
       p.rest_strength_low, p.rest_strength_high,
       p.rest_strength_low, p.rest_strength_high
FROM sbs_progression AS b
JOIN programme AS p ON p.authored_at = COALESCE(
    (SELECT q.authored_at FROM programme AS q WHERE q.authored_at = b.authored_at),
    (SELECT q.authored_at FROM programme AS q ORDER BY q.authored_at DESC LIMIT 1)
);

DROP TABLE sbs_chart_day;
DROP TABLE programme_slot_scheme;
DROP TABLE programme;

PRAGMA foreign_keys = ON;
