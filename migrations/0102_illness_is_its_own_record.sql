CREATE TABLE illness (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at TEXT    NOT NULL,
    from_date   TEXT    NOT NULL UNIQUE,
    ended_on    TEXT    CHECK (ended_on IS NULL OR ended_on >= from_date)
) STRICT;

INSERT INTO illness (authored_at, from_date, ended_on)
    SELECT authored_at, start_date, date(start_date, '+' || days || ' days')
    FROM alteration
    WHERE absence = 'illness';

DELETE FROM alteration WHERE absence = 'illness';

ALTER TABLE alteration_slot RENAME TO alteration_slot_old;
ALTER TABLE alteration      RENAME TO alteration_old;

CREATE TABLE alteration (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at  TEXT    NOT NULL,
    start_date   TEXT    NOT NULL UNIQUE,
    days         INTEGER NOT NULL CHECK (days > 0 AND days <= 255),
    zone         TEXT,
    states_slots INTEGER NOT NULL CHECK (states_slots IN (0, 1)),
    reason       TEXT    NOT NULL CHECK (length(trim(reason)) > 0)
) STRICT;

CREATE TABLE alteration_slot (
    alteration INTEGER NOT NULL REFERENCES alteration(id) ON DELETE CASCADE,
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    part       TEXT    NOT NULL
        CHECK (part IN ('morning', 'afternoon', 'evening')),
    length     TEXT    NOT NULL CHECK (length IN ('shorter', 'longer')),

    PRIMARY KEY (alteration, weekday, part)
) STRICT, WITHOUT ROWID;

INSERT INTO alteration (id, authored_at, start_date, days, zone, states_slots, reason)
    SELECT id, authored_at, start_date, days, zone, states_slots, reason
    FROM alteration_old;

INSERT INTO alteration_slot (alteration, weekday, part, length)
    SELECT alteration, weekday, part, length
    FROM alteration_slot_old;

DROP TABLE alteration_slot_old;
DROP TABLE alteration_old;

CREATE INDEX alteration_by_start ON alteration (start_date);
