CREATE TABLE span_with_job (
    id          INTEGER PRIMARY KEY,
    parent      INTEGER,
    name        TEXT    NOT NULL CHECK (name IN ('command', 'request', 'run', 'step', 'gate', 'check', 'test', 'job')),
    build       TEXT,
    started_at  TEXT    NOT NULL,
    ended_at    TEXT    NOT NULL,
    outcome     TEXT    CHECK (outcome IN ('succeeded', 'failed', 'skipped')),
    fields      TEXT    NOT NULL CHECK (json_valid(fields))
) STRICT;

INSERT INTO span_with_job SELECT id, parent, name, build, started_at, ended_at, outcome, fields FROM span;

DROP TABLE span;

ALTER TABLE span_with_job RENAME TO span;
