CREATE TABLE stress (
    landing_record_id  INTEGER PRIMARY KEY REFERENCES garmin_daily_summary_landing(id),
    source_record_id   TEXT    NOT NULL,
    day                TEXT    NOT NULL,
    average_level      INTEGER NOT NULL CHECK (average_level BETWEEN 1 AND 100),
    measured_seconds   INTEGER NOT NULL CHECK (measured_seconds > 0),
    recorded_by        TEXT,
    endpoint           TEXT    NOT NULL,
    event_kind         TEXT    NOT NULL,
    event_time         TEXT,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

CREATE UNIQUE INDEX stress_by_day ON stress (day);
