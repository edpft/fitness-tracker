-- FTP, derived from the tests that measured it (issue #56, part 3).
--
-- **An interpretive parameter under § 13**, and the constitution names it
-- there: *"Values consulted in order to interpret observations: heart-rate and
-- power zones, FTP, the default timezone. Effect-dated and retained -- the value
-- in force at the time of the observation is the one that applies."*
--
-- **Why it is stored at all**, since it re-derives from the record in
-- milliseconds. The operator, 2026-09-08: *"the main reason we need it is to
-- interpret historical data, to know what zone 2 was when a specific ride was
-- ridden."* That is a different job from prescribing today's session. Today's
-- answer is the only one anyone reads and could be computed on demand; the back
-- catalogue needs a series that is stable across 144 rides going back to
-- 2023-12, because § 6 makes a derivation choice part of a series' method and
-- changing one silently would rewrite every one of them.
--
-- **Rows are rebuilt with the sessions, not appended to.** The operator, same
-- day: *"for now, the FTP table can be re-derived after each normalisation."*
-- So this cannot drift from the record, and needs no rule saying a row is
-- frozen. Freezing would have guarded against a stated average moving, and it
-- cannot: Peloton serves a performance graph exactly once per workout -- 0 of
-- the operator's 427 have been re-served -- so a test's average is fixed the
-- moment it lands.
--
-- **Deliberately not settled here**, and the operator has parked it: *"it
-- doesn't make sense to me for normalisation and derivation to be fully dynamic
-- when the data they are running on is incremental."* He is right that § II
-- requires only that a derivation *equal* a full re-derivation -- it is
-- *"defined by what it is a function of, not by how it is stored"* -- so
-- rebuilding everything is an implementation choice rather than a rule. It costs
-- ~12 seconds on his corpus today. See `docs/roadmap.md`.
--
-- **One row per date.** Two values in force on one day is not a state § 13
-- describes, and the date is what every lookup is by.

CREATE TABLE ftp (
    -- The day the value took effect: the day the test was ridden.
    effect_from  TEXT    PRIMARY KEY,

    -- Whole watts, as the arithmetic produces them. The stated average is a
    -- whole number and 95% of it is rounded, which is what reproduces the
    -- operator's record: 209 W gives 199, where truncating gives 198.
    watts        INTEGER NOT NULL CHECK (watts > 0),

    -- 'estimated' for everything this derivation writes: arithmetic over a
    -- measurement, not a measurement. The twenty minutes were ridden; the hour
    -- they stand for was not. The other two keys are the ones `FtpProvenance`
    -- holds, and nothing writes them yet -- an asserted value has no producer
    -- and, on a record with six tests, nothing that needs one.
    provenance   TEXT    NOT NULL CHECK (provenance IN ('tested', 'estimated', 'asserted')),

    -- The test session this was derived from.
    --
    -- `NOT NULL` because every row here comes from one. A value with no test
    -- behind it is a different thing -- § 13's asserted bootstrap -- and giving
    -- it a nullable column now would be building the shape before there is
    -- anything to put in it.
    measured_by  INTEGER NOT NULL REFERENCES cycling_session(landing_record_id),

    run_id       INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;
