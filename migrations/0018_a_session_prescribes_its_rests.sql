-- How long to rest, block by block.
--
-- **Nothing prescribed rest before this.** Every set went out with
-- `rest_after` absent, which the prescribed model reads as "no instruction
-- given" — true of the warm-up ramp and wrong about everything else.
--
-- Two columns per block, `low` and `high`, and `high` null for a block that
-- states one number rather than a range. That is the same encoding
-- `prescribed_set` already uses for a target, and it is a pair here for the
-- reason it is a pair there: a range is queried by its bounds. The domain holds
-- a minimum and a positive extent instead, and the adapter converts — the
-- `CHECK` below is what makes that conversion total.
--
-- **A rest of zero is a real instruction**, so the checks admit it: a superset
-- tells you to go straight on, and mobility work rests not at all. Only the
-- *extent* of a range must be positive, which is what `high > low` says.
--
-- `after_superset` is nullable throughout, and null does not mean zero. It means
-- the block rests the same however its work is grouped, which is true of every
-- block but the two that state otherwise.
ALTER TABLE generation_parameters ADD COLUMN
    rest_plyometric_low        INTEGER NOT NULL DEFAULT 30 CHECK (rest_plyometric_low >= 0);
ALTER TABLE generation_parameters ADD COLUMN
    rest_plyometric_high       INTEGER CHECK (rest_plyometric_high > rest_plyometric_low);
ALTER TABLE generation_parameters ADD COLUMN
    rest_plyometric_ss_low     INTEGER CHECK (rest_plyometric_ss_low >= 0);
ALTER TABLE generation_parameters ADD COLUMN
    rest_plyometric_ss_high    INTEGER CHECK (rest_plyometric_ss_high > rest_plyometric_ss_low);

ALTER TABLE generation_parameters ADD COLUMN
    rest_power_low             INTEGER NOT NULL DEFAULT 90 CHECK (rest_power_low >= 0);
ALTER TABLE generation_parameters ADD COLUMN
    rest_power_high            INTEGER CHECK (rest_power_high > rest_power_low);
ALTER TABLE generation_parameters ADD COLUMN
    rest_power_ss_low          INTEGER CHECK (rest_power_ss_low >= 0);
ALTER TABLE generation_parameters ADD COLUMN
    rest_power_ss_high         INTEGER CHECK (rest_power_ss_high > rest_power_ss_low);

ALTER TABLE generation_parameters ADD COLUMN
    rest_strength_low          INTEGER NOT NULL DEFAULT 120 CHECK (rest_strength_low >= 0);
ALTER TABLE generation_parameters ADD COLUMN
    rest_strength_high         INTEGER CHECK (rest_strength_high > rest_strength_low);
ALTER TABLE generation_parameters ADD COLUMN
    rest_strength_ss_low       INTEGER CHECK (rest_strength_ss_low >= 0);
ALTER TABLE generation_parameters ADD COLUMN
    rest_strength_ss_high      INTEGER CHECK (rest_strength_ss_high > rest_strength_ss_low);

ALTER TABLE generation_parameters ADD COLUMN
    rest_hypertrophy_low       INTEGER NOT NULL DEFAULT 120 CHECK (rest_hypertrophy_low >= 0);
ALTER TABLE generation_parameters ADD COLUMN
    rest_hypertrophy_high      INTEGER CHECK (rest_hypertrophy_high > rest_hypertrophy_low);
ALTER TABLE generation_parameters ADD COLUMN
    rest_hypertrophy_ss_low    INTEGER CHECK (rest_hypertrophy_ss_low >= 0);
ALTER TABLE generation_parameters ADD COLUMN
    rest_hypertrophy_ss_high   INTEGER CHECK (rest_hypertrophy_ss_high > rest_hypertrophy_ss_low);

ALTER TABLE generation_parameters ADD COLUMN
    rest_mobility_low          INTEGER NOT NULL DEFAULT 0 CHECK (rest_mobility_low >= 0);
ALTER TABLE generation_parameters ADD COLUMN
    rest_mobility_high         INTEGER CHECK (rest_mobility_high > rest_mobility_low);
ALTER TABLE generation_parameters ADD COLUMN
    rest_mobility_ss_low       INTEGER CHECK (rest_mobility_ss_low >= 0);
ALTER TABLE generation_parameters ADD COLUMN
    rest_mobility_ss_high      INTEGER CHECK (rest_mobility_ss_high > rest_mobility_ss_low);

-- The operator's own, stated on 2026-08-23. The rows already in the store were
-- generated before any rest was prescribed, so this is what they would have
-- carried rather than a guess: 0:30 plyometric, 1:30 power, 2:00-3:00 on a
-- strength single and 1:30-2:30 at the end of a superset, hypertrophy the same
-- as strength, nothing on mobility.
UPDATE generation_parameters SET
    rest_plyometric_low      = 30,
    rest_power_low           = 90,
    rest_strength_low        = 120,
    rest_strength_high       = 180,
    rest_strength_ss_low     = 90,
    rest_strength_ss_high    = 150,
    rest_hypertrophy_low     = 120,
    rest_hypertrophy_high    = 180,
    rest_hypertrophy_ss_low  = 90,
    rest_hypertrophy_ss_high = 150,
    rest_mobility_low        = 0;
