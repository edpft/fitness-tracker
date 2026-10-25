CREATE TABLE device_sighting (
    id                 INTEGER PRIMARY KEY,
    stream             TEXT    NOT NULL,
    landing_record_id  INTEGER NOT NULL,
    source_record_id   TEXT    NOT NULL,
    device             TEXT    NOT NULL,
    name               TEXT    NOT NULL CHECK (length(trim(name)) > 0),
    role               TEXT    NOT NULL CHECK (role IN ('recorder', 'paired')),
    worn               INTEGER NOT NULL CHECK (worn IN (0, 1)),
    at_utc             TEXT    NOT NULL,
    zone               TEXT    NOT NULL,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

CREATE INDEX device_sighting_by_stream ON device_sighting (stream);

CREATE TABLE device_sighting_identifier (
    sighting   INTEGER NOT NULL REFERENCES device_sighting(id) ON DELETE CASCADE,
    position   INTEGER NOT NULL CHECK (position >= 0),
    identifier TEXT    NOT NULL,
    PRIMARY KEY (sighting, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE device_sighting_measurand (
    sighting  INTEGER NOT NULL REFERENCES device_sighting(id) ON DELETE CASCADE,
    measurand TEXT    NOT NULL,
    how       TEXT    NOT NULL CHECK (how IN ('measured', 'sent')),
    PRIMARY KEY (sighting, measurand, how)
) STRICT, WITHOUT ROWID;

CREATE TABLE wear_day (
    landing_record_id INTEGER PRIMARY KEY REFERENCES garmin_daily_heart_rate_landing(id),
    source_record_id  TEXT    NOT NULL,
    day               TEXT    NOT NULL,
    worn              INTEGER NOT NULL CHECK (worn IN (0, 1)),
    run_id            INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

CREATE UNIQUE INDEX wear_day_by_day ON wear_day (day);

ALTER TABLE bike_plus_ride ADD COLUMN heart_rate_device TEXT;
ALTER TABLE bike_plus_ride ADD COLUMN heart_rate_device_name TEXT;

ALTER TABLE canonical_bike_plus_ride ADD COLUMN heart_rate_device TEXT;
ALTER TABLE canonical_bike_plus_ride ADD COLUMN heart_rate_device_name TEXT;
ALTER TABLE canonical_bike_plus_ride ADD COLUMN heart_rate_device_normalised_session INTEGER
    REFERENCES cycling_session(landing_record_id);

ALTER TABLE overnight_hrv ADD COLUMN recorded_by TEXT;
ALTER TABLE resting_heart_rate ADD COLUMN recorded_by TEXT;
