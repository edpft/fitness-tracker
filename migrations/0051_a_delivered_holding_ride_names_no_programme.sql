-- no-transaction
PRAGMA foreign_keys = OFF;

CREATE TABLE cycling_delivery_rebuilt (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,

    prescribed_for TEXT    NOT NULL,
    destination    TEXT    NOT NULL
        CHECK (destination <> '' AND destination = lower(destination)),

    programme      TEXT    CHECK (programme IS NULL OR length(trim(programme)) > 0),
    microcycle     INTEGER NOT NULL CHECK (microcycle > 0),
    session        INTEGER NOT NULL CHECK (session > 0),

    delivered_at   TEXT    NOT NULL,

    UNIQUE (prescribed_for, destination)
) STRICT;

INSERT INTO cycling_delivery_rebuilt
    (id, prescribed_for, destination, programme, microcycle, session, delivered_at)
    SELECT id, prescribed_for, destination, programme, microcycle, session, delivered_at
    FROM cycling_delivery;

DROP TABLE cycling_delivery;
ALTER TABLE cycling_delivery_rebuilt RENAME TO cycling_delivery;

PRAGMA foreign_keys = ON;
