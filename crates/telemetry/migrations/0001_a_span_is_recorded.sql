CREATE TABLE span (
    id          INTEGER PRIMARY KEY,
    parent      INTEGER,
    name        TEXT    NOT NULL CHECK (name IN ('command', 'request', 'run', 'step', 'gate', 'check', 'test')),
    build       TEXT,
    started_at  TEXT    NOT NULL,
    ended_at    TEXT    NOT NULL,
    outcome     TEXT    CHECK (outcome IN ('succeeded', 'failed', 'skipped')),
    fields      TEXT    NOT NULL CHECK (json_valid(fields))
) STRICT;
