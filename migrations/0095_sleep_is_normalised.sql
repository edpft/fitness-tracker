CREATE TABLE sleep (
    landing_record_id  INTEGER PRIMARY KEY REFERENCES garmin_sleep_landing(id),
    source_record_id   TEXT    NOT NULL,
    morning_of         TEXT    NOT NULL,
    fell_asleep_utc    TEXT    NOT NULL,
    woke_utc           TEXT    NOT NULL,
    zone               TEXT    NOT NULL,
    asleep_seconds     INTEGER NOT NULL CHECK (asleep_seconds > 0),
    recorded_by        TEXT,
    endpoint           TEXT    NOT NULL,
    event_kind         TEXT    NOT NULL,
    event_time         TEXT,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

CREATE UNIQUE INDEX sleep_by_morning ON sleep (morning_of);
