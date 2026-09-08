-- A ride carries the average power Peloton stated for it (issue #56, part 3).
--
-- **The FTP work is what needed it.** Every value in the operator's record is a
-- twenty-minute test's average output times 0.95, so a derivation over the
-- record has to read an average from somewhere -- and the entity carried
-- distance and nothing else, on the operator's own instruction of 2026-09-07:
-- *"just keep distance in our normalised entity, if we find we want total
-- output or AVG output at some later date, we have the raw data but, for now, I
-- don't see why we'd need them and we could derive them"*. This is that later
-- date, and it arrived with the second half of that sentence overturned.
--
-- **The average is stated, not derived, and the two are different numbers.**
-- The operator asked the question directly, 2026-09-08: *"don't they use average
-- output instead of deriving it from total output divided by duration?"* They
-- do. The performance graph carries `avg_output` beside the per-second series,
-- and across his 231 rides that state one, a mean of those samples disagrees
-- with it on 33 and `total_work / duration` on 59. One ride states 134 watts
-- against a sample mean of 124: its series holds 1,076 values across an index
-- running to 1,800, and an unweighted mean reads the 724 absent seconds as
-- though they had not happened.
--
-- It decides the FTP figures, not just their tidiness. Only the stated average
-- reproduces all six of the operator's known values -- a mean of the samples
-- gets 2024-01-27 and 2024-09-15 wrong by a watt in each direction.
--
-- § 6 makes power method-dependent: the value is inseparable from the sensor
-- and algorithm that produced it, and Peloton's average *is* that algorithm's
-- answer. So this column holds what the source said, and is not a derived
-- metric stored beside its inputs -- which § 5 would forbid.
--
-- **A rebuild rather than an `ALTER`.** SQLite cannot add a `NOT NULL` column
-- without a default, and there is no default to give: a ride whose graph states
-- no average is refused, because inventing one is the whole error above.
--
-- **Rows are not carried, and nothing is lost.** `bike_plus_ride` is a
-- derivation, deleted whole and rewritten by every `fitness normalise
-- peloton.rides` (§ II: never mutated in place). Its children go with it for
-- the same reason. § 7 is what makes this safe: raw holds every workout record
-- and every graph, so the next run rebuilds all of it, with the average this
-- time.

PRAGMA foreign_keys = OFF;

DELETE FROM bike_plus_ride_heart_rate;
DELETE FROM bike_plus_ride_sample;
DELETE FROM bike_plus_ride;
DELETE FROM cycling_session;

CREATE TABLE bike_plus_ride_rebuilt (
    -- The workout record. What identifies the ride, and the only one of the two
    -- responses the source names.
    landing_record_id       INTEGER PRIMARY KEY REFERENCES peloton_ride_landing(id),
    -- The performance graph the samples came from. Not the identity: a graph is
    -- anonymous, and is reachable only through the workout it was fetched for.
    samples_record_id       INTEGER NOT NULL REFERENCES peloton_ride_sample_landing(id),
    source_record_id        TEXT    NOT NULL,

    started_at_utc          TEXT    NOT NULL,
    zone                    TEXT    NOT NULL,

    -- How long the ride lasted, from the workout record's own start and end.
    -- Not the class's length, which Peloton serves separately and the graph
    -- echoes: those are the class's (§ 11), and they differ by up to 82
    -- seconds in the operator's record.
    duration_seconds        INTEGER NOT NULL CHECK (duration_seconds > 0),
    -- Metres, in millimetres, for the reason a load is in grams: the value is
    -- persisted and compared against rows written by earlier versions (§ 7), so
    -- it must not depend on a float's rounding.
    --
    -- **From the graph, never from the workout record.** The workout states a
    -- distance with no unit anywhere and it is miles, following an account
    -- preference; the graph states the same distance and says which unit it is
    -- in. Reading the former as kilometres is a silent 38% error.
    distance_millimetres    INTEGER NOT NULL CHECK (distance_millimetres >= 0),

    -- What the source said the ride averaged, in watts.
    --
    -- Whole watts because that is how Peloton states it: `avg_output` is an
    -- integer on all 279 of the operator's Bike+ rides, where the average speed
    -- beside it carries a decimal. Rounding is the source's, and copying it is
    -- not our loss of precision.
    --
    -- **From `average_summaries`, not from the `output` metric's own
    -- `average_value`.** Peloton serves both and they disagree on 6 of his 231
    -- rides, the metric a watt higher every time. That is one source
    -- contradicting itself about one ride; this is the figure its summary panel
    -- shows.
    average_power_watts     INTEGER NOT NULL CHECK (average_power_watts >= 0),

    -- What the source says it did not measure of the heart-rate series.
    --
    -- Here because there is one per ride, and nullable twice over: null where
    -- the source stated nothing, and null where there is no heart-rate series
    -- at all. The two are told apart by whether `bike_plus_ride_heart_rate`
    -- holds rows for the ride, which is the same fact the entity carries.
    --
    -- Not a count of the gaps in those rows. Peloton counts seconds it held a
    -- value forward for as well as seconds it zeroed, so it is the larger
    -- number and is not recomputable from them.
    heart_rate_declared_missing_seconds INTEGER
                              CHECK (heart_rate_declared_missing_seconds IS NULL
                                     OR heart_rate_declared_missing_seconds >= 0),

    endpoint                TEXT    NOT NULL,
    event_kind              TEXT    NOT NULL,
    event_time              TEXT,
    run_id                  INTEGER NOT NULL REFERENCES normalisation_run(id),

    -- A ride belongs to a session and knows what it was for (0030).
    session                 INTEGER REFERENCES cycling_session(landing_record_id),
    role                    TEXT    CHECK (role IN ('warm-up', 'effort', 'main', 'cool-down'))
) STRICT;

DROP TABLE bike_plus_ride;
ALTER TABLE bike_plus_ride_rebuilt RENAME TO bike_plus_ride;

-- Recreated with the table: an index goes with the table it is on.
CREATE INDEX bike_plus_ride_by_start ON bike_plus_ride (started_at_utc);
CREATE INDEX bike_plus_ride_by_session ON bike_plus_ride (session);

PRAGMA foreign_keys = ON;
