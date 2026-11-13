CREATE TABLE calendar (
    name          TEXT PRIMARY KEY CHECK (name IN ('school', 'public')),
    served        TEXT NOT NULL,
    etag          TEXT,
    last_modified TEXT,
    asked_at      TEXT NOT NULL
) STRICT, WITHOUT ROWID;
