CREATE TABLE cycling_delivery (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,

    prescribed_for TEXT    NOT NULL,
    destination    TEXT    NOT NULL
        CHECK (destination <> '' AND destination = lower(destination)),

    programme      TEXT    NOT NULL CHECK (length(trim(programme)) > 0),
    microcycle     INTEGER NOT NULL CHECK (microcycle > 0),
    session        INTEGER NOT NULL CHECK (session > 0),

    delivered_at   TEXT    NOT NULL,

    UNIQUE (prescribed_for, destination)
) STRICT;

CREATE TABLE cycling_delivery_class (
    delivery  INTEGER NOT NULL REFERENCES cycling_delivery(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL CHECK (position >= 0),

    reference TEXT    NOT NULL CHECK (length(trim(reference)) > 0),
    called    TEXT    NOT NULL CHECK (length(trim(called)) > 0),

    PRIMARY KEY (delivery, position)
) STRICT, WITHOUT ROWID;
