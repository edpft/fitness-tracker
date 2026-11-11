CREATE TABLE padel_game (
    landing_record_id  INTEGER PRIMARY KEY REFERENCES garmin_activity_landing(id),
    source_record_id   TEXT    NOT NULL,
    started_at_utc     TEXT    NOT NULL,
    zone               TEXT    NOT NULL,
    duration_seconds   INTEGER NOT NULL CHECK (duration_seconds > 0),
    average_bpm        INTEGER CHECK (average_bpm IS NULL OR average_bpm > 0),
    highest_bpm        INTEGER CHECK (highest_bpm IS NULL OR highest_bpm > 0),
    endpoint           TEXT    NOT NULL,
    event_kind         TEXT    NOT NULL,
    event_time         TEXT,
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id),
    CHECK ((average_bpm IS NULL) = (highest_bpm IS NULL))
) STRICT;

CREATE INDEX padel_game_by_start ON padel_game (started_at_utc);
