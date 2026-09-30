CREATE TABLE measured_gym_session_heart_rate (
    workout           INTEGER NOT NULL REFERENCES gym_workout(id),
    at_seconds        INTEGER NOT NULL,
    beats_per_minute  INTEGER NOT NULL CHECK (beats_per_minute > 0),

    PRIMARY KEY (workout, at_seconds),
    CHECK (at_seconds >= 0)
) STRICT, WITHOUT ROWID;

ALTER TABLE measured_gym_session
    ADD COLUMN recording_landing_record_id INTEGER;
