CREATE TABLE peloton_class (
    reference  TEXT    PRIMARY KEY CHECK (length(trim(reference)) > 0),

    title      TEXT    NOT NULL CHECK (length(trim(title)) > 0),
    duration   INTEGER NOT NULL CHECK (duration > 0),
    series     TEXT    CHECK (series IS NULL OR length(trim(series)) > 0),
    instructor TEXT    CHECK (instructor IS NULL OR length(trim(instructor)) > 0),
    aired_at   INTEGER,

    listed_at  TEXT    NOT NULL,

    detail     TEXT    CHECK (detail IS NULL OR length(detail) > 0),
    read_at    TEXT,

    CHECK ((detail IS NULL) = (read_at IS NULL))
) STRICT;

CREATE INDEX peloton_class_unread
    ON peloton_class (aired_at DESC)
 WHERE detail IS NULL;
