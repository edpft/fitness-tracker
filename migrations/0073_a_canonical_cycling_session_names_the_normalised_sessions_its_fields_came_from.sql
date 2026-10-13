CREATE TABLE canonical_cycling_session (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    kind  TEXT    NOT NULL CHECK (kind IN ('ride', 'test'))
) STRICT;

CREATE TABLE canonical_bike_plus_ride (
    session                     INTEGER NOT NULL REFERENCES canonical_cycling_session(id),
    position                    INTEGER NOT NULL CHECK (position >= 0),
    role                        TEXT    NOT NULL
        CHECK (role IN ('warm-up', 'effort', 'main', 'cool-down')),
    started_at_utc              TEXT    NOT NULL,
    zone                        TEXT    NOT NULL,
    started_at_normalised_session INTEGER NOT NULL REFERENCES cycling_session(landing_record_id),
    venue_reference             TEXT    NOT NULL CHECK (length(trim(venue_reference)) > 0),
    venue_called                TEXT    NOT NULL CHECK (length(trim(venue_called)) > 0),
    venue_normalised_session    INTEGER NOT NULL REFERENCES cycling_session(landing_record_id),
    duration_seconds            INTEGER NOT NULL CHECK (duration_seconds > 0),
    duration_normalised_session INTEGER NOT NULL REFERENCES cycling_session(landing_record_id),
    distance_millimetres        INTEGER NOT NULL CHECK (distance_millimetres >= 0),
    distance_normalised_session INTEGER NOT NULL REFERENCES cycling_session(landing_record_id),
    average_power_watts         INTEGER NOT NULL CHECK (average_power_watts >= 0),
    average_power_normalised_session INTEGER NOT NULL REFERENCES cycling_session(landing_record_id),
    samples_normalised_session  INTEGER NOT NULL REFERENCES cycling_session(landing_record_id),
    heart_rate_normalised_session INTEGER REFERENCES cycling_session(landing_record_id),
    heart_rate_declared_missing_seconds INTEGER
        CHECK (heart_rate_declared_missing_seconds IS NULL
               OR heart_rate_declared_missing_seconds >= 0),
    heart_rate_declared_missing_normalised_session INTEGER
        REFERENCES cycling_session(landing_record_id),
    PRIMARY KEY (session, position),
    CHECK ((heart_rate_declared_missing_seconds IS NULL)
           = (heart_rate_declared_missing_normalised_session IS NULL))
) STRICT, WITHOUT ROWID;

CREATE TABLE canonical_bike_plus_ride_sample (
    session                    INTEGER NOT NULL,
    ride_position              INTEGER NOT NULL,
    at_seconds                 INTEGER NOT NULL CHECK (at_seconds >= 0),
    power_watts                INTEGER NOT NULL CHECK (power_watts >= 0),
    cadence_rpm                INTEGER NOT NULL CHECK (cadence_rpm >= 0),
    resistance_percentage      INTEGER NOT NULL CHECK (resistance_percentage BETWEEN 0 AND 100),
    speed_millimetres_per_hour INTEGER NOT NULL CHECK (speed_millimetres_per_hour >= 0),
    PRIMARY KEY (session, ride_position, at_seconds),
    FOREIGN KEY (session, ride_position) REFERENCES canonical_bike_plus_ride(session, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE canonical_bike_plus_ride_heart_rate (
    session          INTEGER NOT NULL,
    ride_position    INTEGER NOT NULL,
    at_seconds       INTEGER NOT NULL CHECK (at_seconds >= 0),
    beats_per_minute INTEGER NOT NULL CHECK (beats_per_minute > 0),
    PRIMARY KEY (session, ride_position, at_seconds),
    FOREIGN KEY (session, ride_position) REFERENCES canonical_bike_plus_ride(session, position)
) STRICT, WITHOUT ROWID;

CREATE INDEX canonical_bike_plus_ride_by_start ON canonical_bike_plus_ride (started_at_utc);
