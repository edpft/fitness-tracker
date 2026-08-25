-- When the operator has room to train, and what departs from it.
--
-- `domain::schedule` has held these types since #22 with nothing to persist
-- them. This is where they live.
--
-- **A training slot is a time; an exercise slot is a position in a session.**
-- Both are things to be filled, which is why both were called a slot. Only one
-- of them is in this file.
--
-- **Two shapes, two tables, and no foreign key between them.** An alteration
-- is a fact about dates, not about which pattern happened to be in force when
-- it was recorded -- so hanging one off a pattern would lose every alteration
-- already recorded the next time the ordinary pattern changed. `Diary`
-- assembles them by date, which is the only thing that relates them.
--
-- An alteration is not a holiday: a course, a visitor and a late finish all
-- change a week without being a trip.
--
-- **A pattern is never superseded by a flag.** It is in force from its date
-- until one with a later date exists, so every row stays and `Diary::on` takes
-- the last that applies. An end date would be a second place for the same fact,
-- and the two could disagree.

CREATE TABLE training_pattern (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at  TEXT    NOT NULL,

    -- In force from this date. Open-ended by construction: a routine does not
    -- have an end, it has a successor.
    from_date    TEXT    NOT NULL UNIQUE,
    zone         TEXT    NOT NULL
) STRICT;

-- A row per slot, and each says whose it is.
--
-- **The allocation lives with the slot, not beside it.** Which slots are the
-- gym's and which are cycling's depends on the alterations -- a holiday that
-- turns two weekday evenings into a Saturday morning has to say who gets the
-- Saturday -- so anything holding the allocation apart from the slots would
-- need to know about alterations too. `discipline` is the activity and never
-- the vendor: cycling is cycling whatever records it.
CREATE TABLE training_slot (
    pattern    INTEGER NOT NULL REFERENCES training_pattern(id) ON DELETE CASCADE,
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    part       TEXT    NOT NULL
        CHECK (part IN ('morning', 'afternoon', 'evening')),
    discipline TEXT    NOT NULL CHECK (discipline IN ('gym', 'cycling')),

    PRIMARY KEY (pattern, weekday, part)
) STRICT, WITHOUT ROWID;

-- A run of days that departs from the ordinary pattern.
--
-- Keyed on the day it starts, so re-stating the holiday that begins on the 14th
-- corrects it rather than booking a second one. Two patches may still overlap --
-- a long trip with a different arrangement in the middle of it is an ordinary
-- thing to describe -- and `Diary::on` gives the last of them the final word.
CREATE TABLE alteration (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    authored_at  TEXT    NOT NULL,

    start_date   TEXT    NOT NULL UNIQUE,
    days         INTEGER NOT NULL CHECK (days > 0 AND days <= 255),

    -- Null is "the zone is unchanged", which is not the same as any zone.
    zone         TEXT,

    -- **The one column that exists because null is already taken.** An alteration's
    -- slots are `Option<BTreeSet<Slot>>`: absent means "the ordinary week
    -- stands", and present-but-empty means "no room to train at all". Both
    -- would be zero rows in `alteration_slot`, so the distinction has to be
    -- carried here -- and collapsing them would make a zone-only patch cancel
    -- every session of the trip.
    states_slots INTEGER NOT NULL CHECK (states_slots IN (0, 1)),

    -- Why. An unexplained override is unreadable six months later.
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

-- An alteration that states no slots has no rows here, and so does one that states
-- the empty set. `states_slots` is what tells them apart.
CREATE INDEX alteration_by_start ON alteration (start_date);
