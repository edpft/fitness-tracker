CREATE TABLE peloton_cool_down (
    reference  TEXT    PRIMARY KEY CHECK (length(trim(reference)) > 0),
    title      TEXT    NOT NULL CHECK (length(trim(title)) > 0),
    instructor TEXT    CHECK (instructor IS NULL OR length(trim(instructor)) > 0),
    aired_at   INTEGER,
    listed_at  TEXT    NOT NULL
) STRICT;

CREATE INDEX peloton_cool_down_newest
    ON peloton_cool_down (instructor, aired_at DESC);
