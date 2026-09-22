CREATE TABLE gym_closure (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at TEXT    NOT NULL,
    start_date  TEXT    NOT NULL UNIQUE,
    days        INTEGER NOT NULL CHECK (days > 0 AND days <= 255),
    reason      TEXT    NOT NULL CHECK (length(trim(reason)) > 0)
) STRICT;
