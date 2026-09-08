-- The normalised layer's second entity: a ride on a Peloton Bike+.
--
-- **Three tables, and the reason there are three is § 6.** A ride's summary is
-- one row; the four series the bike produced are one row per second, because
-- the bike produced them together at one index by one method; the heart rate is
-- its own table, because it is the one measurement here that is not the bike's.
-- It comes from a watch on broadcast relayed through Peloton, which § 6 says
-- makes it a different series even inside one entity.
--
-- That split is not tidiness. As a nullable column on the sample row, "nothing
-- was worn" and "the strap dropped out for every second of the ride" would be
-- the same rows of nulls. As its own table they are no rows and — since a
-- series with no readings cannot be built — no ride either.
--
-- **The ride names two landing records** (§ 3.1, constitution 3.1.0). Peloton
-- serves a ride's start and duration from its workout list and that ride's
-- samples from a performance graph fetched per workout; the graph names no
-- workout, states no time, no zone and no device, and neither response is an
-- entity alone. Nothing is being reconciled, because the two do not overlap.
--
-- Keyed on the landing record rather than on `source_record_id`, exactly as
-- `gym_workout` is: two records sharing a source id are the same source
-- contradicting itself, § 10 puts that at the canonical layer, and keying on
-- the source id here would collapse the pair silently. The operator's account
-- holds 24 such pairs.

CREATE TABLE bike_plus_ride (
    -- The workout record. What identifies the ride, and the only one of the two
    -- responses the source names.
    landing_record_id       INTEGER PRIMARY KEY REFERENCES peloton_workout_landing(id),
    -- The performance graph the samples came from. Not the identity: a graph is
    -- anonymous, and is reachable only through the workout it was fetched for.
    samples_record_id       INTEGER NOT NULL REFERENCES peloton_workout_sample_landing(id),
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
    run_id                  INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

-- One second of what the bike measured.
--
-- `at_seconds` is carried rather than implied by row order. The index Peloton
-- serves is genuinely sparse — on 143 of the operator's 285 rides it starts at
-- 4 or 5, or skips 38 seconds in the middle — so ordinal positions would
-- silently restate a gap as continuous recording, which § 37 forbids.
--
-- Speed is in millimetres per hour rather than per second, and it is the one
-- quantity here that does not canonicalise to metres and seconds. It arrives as
-- kilometres per hour to one decimal place and dividing by 3.6 is not exact;
-- since the value is persisted, the hour stays and the metre is subdivided.
CREATE TABLE bike_plus_ride_sample (
    ride                    INTEGER NOT NULL REFERENCES bike_plus_ride(landing_record_id),
    at_seconds              INTEGER NOT NULL,

    power_watts             INTEGER NOT NULL CHECK (power_watts >= 0),
    cadence_rpm             INTEGER NOT NULL CHECK (cadence_rpm >= 0),
    resistance_percentage   INTEGER NOT NULL
                              CHECK (resistance_percentage BETWEEN 0 AND 100),
    speed_millimetres_per_hour INTEGER NOT NULL CHECK (speed_millimetres_per_hour >= 0),

    PRIMARY KEY (ride, at_seconds)
) STRICT, WITHOUT ROWID;

-- One second of what the watch broadcast.
--
-- A row exists only where there was a reading. Peloton serves a 0 for every
-- second the watch was silent — 4,528 of them across the operator's record,
-- 2,037 consecutively on one ride — and a heart does not beat zero times a
-- minute while its owner is pedalling. Storing those would make an average over
-- the series answer 31 bpm for a ride ridden at 128.
--
-- What the source declares it missed sits on the ride rather than here, because
-- it is a total rather than a position.
CREATE TABLE bike_plus_ride_heart_rate (
    ride                    INTEGER NOT NULL REFERENCES bike_plus_ride(landing_record_id),
    at_seconds              INTEGER NOT NULL,
    beats_per_minute        INTEGER NOT NULL CHECK (beats_per_minute > 0),

    PRIMARY KEY (ride, at_seconds)
) STRICT, WITHOUT ROWID;

CREATE INDEX bike_plus_ride_by_start ON bike_plus_ride (started_at_utc);

-- Refusals stop being Hevy's.
--
-- `normalisation_refusal` had no `stream` column and a foreign key into
-- `hevy_workout_landing`, which was invisible while one stream derived: the
-- store filtered by nothing because there was nothing to filter, and
-- `SqliteRefusalStore::replace` emptied the table. With a second stream
-- deriving, that is one derivation deleting another's refusals and a Peloton
-- record failing a constraint that names the wrong table.
--
-- The key goes rather than becoming a pair of nullable keys. A refusal names a
-- landing record in whichever table its stream lands in, and the stream says
-- which — a polymorphic reference SQLite cannot enforce, in exchange for a
-- table that does not gain a column per source.

CREATE TABLE normalisation_refusal_next (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id),
    -- Which stream's derivation produced this, and therefore which landing
    -- table `landing_record_id` points into.
    stream             TEXT    NOT NULL,
    landing_record_id  INTEGER NOT NULL,
    source_record_id   TEXT    NOT NULL,

    locus_kind         TEXT    NOT NULL
        CHECK (locus_kind IN ('record', 'entry', 'set', 'grouping')),
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
           OR (group_id IS NOT NULL AND entry_index IS NULL AND set_index IS NULL))
) STRICT;

-- Every existing row is Hevy's, because it is the only stream that has ever
-- derived. The rows are carried rather than dropped: a refusal is what the last
-- derivation would not accept, and re-deriving to get them back would be a
-- migration that quietly needs a command run after it.
INSERT INTO normalisation_refusal_next (
    id, run_id, stream, landing_record_id, source_record_id, locus_kind,
    entry_index, set_index, group_id, exercise, reason, kind, detail
)
SELECT
    id, run_id, 'hevy.workouts', landing_record_id, source_record_id, locus_kind,
    entry_index, set_index, group_id, exercise, reason, kind, detail
FROM normalisation_refusal;

DROP TABLE normalisation_refusal;

ALTER TABLE normalisation_refusal_next RENAME TO normalisation_refusal;

CREATE INDEX normalisation_refusal_by_kind
    ON normalisation_refusal (kind, reason);

CREATE INDEX normalisation_refusal_by_stream
    ON normalisation_refusal (stream, id);
