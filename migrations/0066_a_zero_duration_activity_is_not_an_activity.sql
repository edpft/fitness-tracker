CREATE TABLE measured_gym_session_rebuilt (
    workout                     INTEGER PRIMARY KEY REFERENCES gym_workout(id),
    duration_seconds            INTEGER NOT NULL,
    average_bpm                 INTEGER,
    highest_bpm                 INTEGER,
    sets_landing_record_id      INTEGER,
    recording_landing_record_id INTEGER,
    CHECK (duration_seconds > 0),
    CHECK ((average_bpm IS NULL) = (highest_bpm IS NULL)),
    CHECK (average_bpm IS NULL OR average_bpm > 0),
    CHECK (highest_bpm IS NULL OR highest_bpm > 0)
) STRICT;

INSERT INTO measured_gym_session_rebuilt (
    workout, duration_seconds, average_bpm, highest_bpm,
    sets_landing_record_id, recording_landing_record_id
)
SELECT workout, duration_seconds, average_bpm, highest_bpm,
       sets_landing_record_id, recording_landing_record_id
FROM measured_gym_session
WHERE duration_seconds > 0;

DROP TABLE measured_gym_session;

ALTER TABLE measured_gym_session_rebuilt RENAME TO measured_gym_session;
