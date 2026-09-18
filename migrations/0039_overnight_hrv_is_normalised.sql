CREATE TABLE overnight_hrv (
    landing_record_id           INTEGER PRIMARY KEY REFERENCES garmin_hrv_landing(id),
    source_record_id            TEXT    NOT NULL,
    morning_of                  TEXT    NOT NULL,
    measured_from_utc           TEXT    NOT NULL,
    measured_until_utc          TEXT    NOT NULL,
    zone                        TEXT    NOT NULL,
    last_night_average_ms       INTEGER NOT NULL CHECK (last_night_average_ms > 0),
    last_night_five_minute_high_ms INTEGER NOT NULL CHECK (last_night_five_minute_high_ms > 0),
    weekly_average_ms           INTEGER NOT NULL CHECK (weekly_average_ms > 0),
    status                      TEXT    NOT NULL
        CHECK (status IN ('balanced', 'unbalanced', 'low')),
    baseline_low_upper_ms       INTEGER NOT NULL CHECK (baseline_low_upper_ms > 0),
    baseline_balanced_low_ms    INTEGER NOT NULL CHECK (baseline_balanced_low_ms > 0),
    baseline_balanced_upper_ms  INTEGER NOT NULL CHECK (baseline_balanced_upper_ms > 0),
    endpoint                    TEXT    NOT NULL,
    event_kind                  TEXT    NOT NULL,
    event_time                  TEXT,
    run_id                      INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

CREATE UNIQUE INDEX overnight_hrv_by_morning ON overnight_hrv (morning_of);

CREATE TABLE overnight_hrv_reading (
    night         INTEGER NOT NULL REFERENCES overnight_hrv(landing_record_id),
    taken_at_utc  TEXT    NOT NULL,
    zone          TEXT    NOT NULL,
    value_ms      INTEGER NOT NULL CHECK (value_ms > 0),
    PRIMARY KEY (night, taken_at_utc)
) STRICT, WITHOUT ROWID;
