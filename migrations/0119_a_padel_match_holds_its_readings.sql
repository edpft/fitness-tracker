CREATE TABLE padel_match_heart_rate (
    landing_record_id  INTEGER NOT NULL REFERENCES padel_match(landing_record_id),
    at_seconds         INTEGER NOT NULL,
    beats_per_minute   INTEGER NOT NULL CHECK (beats_per_minute > 0),

    PRIMARY KEY (landing_record_id, at_seconds),
    CHECK (at_seconds >= 0)
) STRICT, WITHOUT ROWID;
