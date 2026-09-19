ALTER TABLE alteration_slot RENAME TO alteration_slot_old;
ALTER TABLE alteration      RENAME TO alteration_old;

CREATE TABLE alteration (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at  TEXT    NOT NULL,
    start_date   TEXT    NOT NULL UNIQUE,
    days         INTEGER NOT NULL CHECK (days > 0 AND days <= 255),
    absence      TEXT    NOT NULL CHECK (absence IN ('holiday', 'illness')),
    zone         TEXT    CHECK (zone IS NULL OR absence = 'holiday'),
    reason       TEXT    NOT NULL CHECK (length(trim(reason)) > 0)
) STRICT;

CREATE TABLE alteration_slot (
    alteration INTEGER NOT NULL REFERENCES alteration(id) ON DELETE CASCADE,
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    part       TEXT    NOT NULL
        CHECK (part IN ('morning', 'afternoon', 'evening')),
    discipline TEXT    NOT NULL CHECK (discipline IN ('gym', 'cycling')),

    PRIMARY KEY (alteration, weekday, part)
) STRICT, WITHOUT ROWID;

INSERT INTO alteration (id, authored_at, start_date, days, absence, zone, reason)
    SELECT id, authored_at, start_date, days, 'holiday', zone, reason
    FROM alteration_old
    WHERE states_slots = 1;

INSERT INTO alteration_slot (alteration, weekday, part, discipline)
    SELECT alteration, weekday, part, discipline
    FROM alteration_slot_old
    WHERE alteration IN (SELECT id FROM alteration);

DROP TABLE alteration_slot_old;
DROP TABLE alteration_old;

CREATE INDEX alteration_by_start ON alteration (start_date);
