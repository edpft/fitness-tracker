CREATE TABLE resting_heart_rate (
    landing_record_id  INTEGER PRIMARY KEY REFERENCES garmin_daily_heart_rate_landing(id),
    source_record_id   TEXT    NOT NULL,
    day                TEXT    NOT NULL,
    rate_bpm           INTEGER NOT NULL CHECK (rate_bpm > 0),
    endpoint           TEXT    NOT NULL,
    event_kind         TEXT    NOT NULL,
    event_time         TEXT,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

CREATE UNIQUE INDEX resting_heart_rate_by_day ON resting_heart_rate (day);
