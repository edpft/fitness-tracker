-- The operator's week, and the holidays that depart from it.
--
-- `domain::schedule` has held these types since #22 with nothing to persist
-- them. This is where they live.
--
-- **Two shapes, two tables, and no foreign key between them.** A patch is a
-- fact about dates, not about which ordinary week happened to be in force when
-- it was booked -- so hanging a holiday off a schedule would lose every booked
-- holiday the next time the ordinary week changed. `Diary` assembles them by
-- date, which is the only thing that relates them.
--
-- **A schedule is never superseded by a flag.** It is in force from its date
-- until one with a later date exists, so every row stays and `Diary::on` takes
-- the last that applies. An end date would be a second place for the same fact,
-- and the two could disagree.

CREATE TABLE schedule (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at  TEXT    NOT NULL,

    -- In force from this date. Open-ended by construction: a routine does not
    -- have an end, it has a successor.
    from_date    TEXT    NOT NULL UNIQUE,
    zone         TEXT    NOT NULL
) STRICT;

-- A row per slot, which is what a set of slots is.
CREATE TABLE schedule_slot (
    schedule  INTEGER NOT NULL REFERENCES schedule(id) ON DELETE CASCADE,
    weekday   TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    part      TEXT    NOT NULL
        CHECK (part IN ('morning', 'afternoon', 'evening')),

    PRIMARY KEY (schedule, weekday, part)
) STRICT, WITHOUT ROWID;

-- A run of days that departs from the ordinary week.
--
-- Keyed on the day it starts, so re-stating the holiday that begins on the 14th
-- corrects it rather than booking a second one. Two patches may still overlap --
-- a long trip with a different arrangement in the middle of it is an ordinary
-- thing to describe -- and `Diary::on` gives the last of them the final word.
CREATE TABLE schedule_patch (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at  TEXT    NOT NULL,

    start_date   TEXT    NOT NULL UNIQUE,
    days         INTEGER NOT NULL CHECK (days > 0 AND days <= 255),

    -- Null is "the zone is unchanged", which is not the same as any zone.
    zone         TEXT,

    -- **The one column that exists because null is already taken.** A patch's
    -- slots are `Option<BTreeSet<Slot>>`: absent means "the ordinary week
    -- stands", and present-but-empty means "no room to train at all". Both
    -- would be zero rows in `schedule_patch_slot`, so the distinction has to be
    -- carried here -- and collapsing them would make a zone-only patch cancel
    -- every session of the trip.
    states_slots INTEGER NOT NULL CHECK (states_slots IN (0, 1)),

    -- Why. An unexplained override is unreadable six months later.
    reason       TEXT    NOT NULL CHECK (length(trim(reason)) > 0)
) STRICT;

CREATE TABLE schedule_patch_slot (
    patch    INTEGER NOT NULL REFERENCES schedule_patch(id) ON DELETE CASCADE,
    weekday  TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    part     TEXT    NOT NULL
        CHECK (part IN ('morning', 'afternoon', 'evening')),

    PRIMARY KEY (patch, weekday, part)
) STRICT, WITHOUT ROWID;

-- A patch that states no slots has no rows here, and so does one that states
-- the empty set. `states_slots` is what tells them apart.
CREATE INDEX schedule_patch_by_start ON schedule_patch (start_date);
