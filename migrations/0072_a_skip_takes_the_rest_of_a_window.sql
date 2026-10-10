CREATE TABLE skip (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at TEXT    NOT NULL,
    from_date   TEXT    NOT NULL,
    from_part   TEXT    NOT NULL
        CHECK (from_part IN ('morning', 'afternoon', 'evening')),
    until_date  TEXT    NOT NULL,
    until_part  TEXT    NOT NULL
        CHECK (until_part IN ('morning', 'afternoon', 'evening')),
    reason      TEXT    NOT NULL CHECK (length(trim(reason)) > 0)
) STRICT;

CREATE INDEX skip_by_until ON skip (until_date, until_part);
