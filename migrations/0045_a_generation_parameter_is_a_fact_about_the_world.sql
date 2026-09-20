-- no-transaction
PRAGMA foreign_keys = OFF;

CREATE TABLE generation_parameters_rebuilt (
    authored_at                TEXT    PRIMARY KEY,

    light_of_heavy_bp          INTEGER NOT NULL CHECK (light_of_heavy_bp > 0),
    ladder_climb_grams         INTEGER NOT NULL CHECK (ladder_climb_grams > 0),
    entry_drop_bp              INTEGER NOT NULL CHECK (entry_drop_bp < 0),
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

INSERT INTO generation_parameters_rebuilt (
    authored_at, light_of_heavy_bp, ladder_climb_grams, entry_drop_bp,
    static_hold_seconds,
    rest_plyometric_low, rest_plyometric_high, rest_plyometric_ss_low, rest_plyometric_ss_high,
    rest_power_low, rest_power_high, rest_power_ss_low, rest_power_ss_high,
    rest_strength_low, rest_strength_high, rest_strength_ss_low, rest_strength_ss_high,
    rest_hypertrophy_low, rest_hypertrophy_high, rest_hypertrophy_ss_low, rest_hypertrophy_ss_high,
    rest_mobility_low, rest_mobility_high, rest_mobility_ss_low, rest_mobility_ss_high
)
SELECT
    authored_at, light_of_heavy_bp, ladder_climb_grams, entry_drop_bp,
    static_hold_seconds,
    rest_plyometric_low, rest_plyometric_high, rest_plyometric_ss_low, rest_plyometric_ss_high,
    rest_power_low, rest_power_high, rest_power_ss_low, rest_power_ss_high,
    rest_strength_low, rest_strength_high, rest_strength_ss_low, rest_strength_ss_high,
    rest_hypertrophy_low, rest_hypertrophy_high, rest_hypertrophy_ss_low, rest_hypertrophy_ss_high,
    rest_mobility_low, rest_mobility_high, rest_mobility_ss_low, rest_mobility_ss_high
FROM generation_parameters;

DROP TABLE generation_warmup_step;
DROP TABLE generation_role_reps;
DROP TABLE generation_parameters;
ALTER TABLE generation_parameters_rebuilt RENAME TO generation_parameters;

PRAGMA foreign_keys = ON;