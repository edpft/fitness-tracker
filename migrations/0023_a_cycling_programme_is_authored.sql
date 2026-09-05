-- A cycling programme, authored and kept (§ 12).
--
-- Until now `cycling` appeared in the schema only as a `discipline` value on a
-- training slot: `cycling next` took its start date as a flag on every run and
-- read a programme compiled into the binary. The gym authors and remembers;
-- cycling recomputed from a flag.
--
-- **Its own tables rather than a fifth `programme.template`, and the reason is
-- not the CHECK.** Relaxing that CHECK costs nine tables (see 0007), but the
-- rule that actually rules it out is one ring up: `Authoring::author` refuses
-- two programmes covering one day, because two programmes answering for one
-- date would make which of them answers depend on the order rows came back in.
-- Cycling and the gym *must* cover the same days — running them together is
-- what this tool is for — so a cycling row in `programme` would be refused by
-- the very rule that keeps the gym's succession honest, or would force that
-- rule to grow a discipline column and stop being one rule. Two tables, two
-- sets, one rule applied to each.
--
-- Everything gym-shaped in `programme` is absent here and nothing replaces it:
-- there is no primary lift, no anchor, no gating role and no template. A
-- cycling programme is microcycles of rides, and what varies between two of
-- them is the rides.
--
-- **One authored programme per mesocycle**, the way the gym authors one per SBS
-- cycle. Decision 0026 is why it is not one thirteen-week row: a mesocycle taken
-- from Power Zone Build and one taken from Peak Your Power Zones are two
-- published programmes with their own vocabulary, and a row spanning both would
-- mix bounded contexts.

-- The mesocycle. `start_date` is the Monday microcycle one begins on, and the
-- weeks it occupies are counted from the microcycles that exist rather than
-- stated -- a duration column would be a second place for a fact the rows below
-- already carry, and the two could disagree.
CREATE TABLE cycling_programme (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT    NOT NULL,
    authored_at  TEXT    NOT NULL,
    start_date   TEXT    NOT NULL,

    -- One authoring of one programme, as `programme` has it.
    UNIQUE (name, authored_at)
) STRICT;

-- Which weekday rides which session of the microcycle.
--
-- The cycling counterpart of `programme_weekday`, and ordinal where that one is
-- named: the gym's two sessions are a light and a heavy, and a published cycling
-- programme's are a first, a second and a third.
--
-- A session is ridden once a week and a weekday rides once, so both are unique.
CREATE TABLE cycling_weekday (
    programme  INTEGER NOT NULL REFERENCES cycling_programme(id) ON DELETE CASCADE,
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    session    INTEGER NOT NULL CHECK (session > 0),

    PRIMARY KEY (programme, weekday),
    UNIQUE (programme, session)
) STRICT, WITHOUT ROWID;

-- One week of the programme, and which published microcycle it was taken from.
--
-- **The published numbering, never this programme's.** An answer of µ1-2-4-5
-- keeps the four numbers the published programme itself uses, so the third
-- microcycle here says `microcycle = 4`. That is the way back to what was not
-- chosen: a re-authoring that wants the week's third session knows which
-- microcycle of which programme to ask for.
CREATE TABLE cycling_microcycle (
    programme          INTEGER NOT NULL REFERENCES cycling_programme(id) ON DELETE CASCADE,
    -- This programme's own order, from one.
    ordinal            INTEGER NOT NULL CHECK (ordinal > 0),

    published          TEXT    NOT NULL,
    published_ordinal  INTEGER NOT NULL CHECK (published_ordinal > 0),

    PRIMARY KEY (programme, ordinal)
) STRICT, WITHOUT ROWID;

-- One ride: the class's own warm-up and cool-down, and the working part.
--
-- **The zone plan is stored, not re-derived.** Re-fetching a class is neither
-- free nor always available, and § 13 wants a prescription issued last month to
-- stay reproducible -- so `cycling_interval` below holds the ride in full and
-- nothing reads the network to answer what is next.
--
-- **A ride is intervals or an effort, and never both.** An FTP test is a
-- duration with no zone attached, because a zone is a share of the number that
-- ride measures. `effort_seconds` non-null is that case, and the CHECK is what
-- keeps the two from being asserted at once -- the intervals it excludes are
-- checked in the reader, which is the only place that can count rows.
--
-- **Absent and zero are different claims** for the cool-down: the FTP test ships
-- with no cool-down section at all, and a zero-length one would be this side
-- inventing a section the class does not have.
CREATE TABLE cycling_ride (
    programme       INTEGER NOT NULL REFERENCES cycling_programme(id) ON DELETE CASCADE,
    microcycle      INTEGER NOT NULL CHECK (microcycle > 0),
    session         INTEGER NOT NULL CHECK (session > 0),

    warm_up_seconds INTEGER NOT NULL CHECK (warm_up_seconds > 0),
    cool_down_seconds INTEGER CHECK (cool_down_seconds IS NULL OR cool_down_seconds > 0),
    effort_seconds  INTEGER CHECK (effort_seconds IS NULL OR effort_seconds > 0),

    PRIMARY KEY (programme, microcycle, session),
    FOREIGN KEY (programme, microcycle)
        REFERENCES cycling_microcycle(programme, ordinal) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

-- Where a ride is done, in the order it is ridden.
--
-- **A session is one or more places** (decision 0033): the FTP warm-up and the
-- test itself are two classes and one session. `reference` is the destination's
-- own identifier and is not interpreted here or anywhere above this adapter --
-- it is a Peloton class id today and the column says nothing about that. `called`
-- is what the destination calls it, carried so a prescription can print it
-- without a fetch; it identifies nothing, because two classes really do share a
-- title.
CREATE TABLE cycling_venue (
    programme   INTEGER NOT NULL REFERENCES cycling_programme(id) ON DELETE CASCADE,
    microcycle  INTEGER NOT NULL,
    session     INTEGER NOT NULL,
    position    INTEGER NOT NULL CHECK (position >= 0),

    reference   TEXT    NOT NULL CHECK (length(trim(reference)) > 0),
    called      TEXT    NOT NULL CHECK (length(trim(called)) > 0),

    PRIMARY KEY (programme, microcycle, session, position),
    FOREIGN KEY (programme, microcycle, session)
        REFERENCES cycling_ride(programme, microcycle, session) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

-- The working part of a ride: zones in order, each for a stated time.
--
-- Seconds rather than a duration, and a zone number rather than a name, for the
-- reason loads are grams: a stored prescription that cannot be reproduced
-- exactly is not a record of anything.
CREATE TABLE cycling_interval (
    programme   INTEGER NOT NULL REFERENCES cycling_programme(id) ON DELETE CASCADE,
    microcycle  INTEGER NOT NULL,
    session     INTEGER NOT NULL,
    position    INTEGER NOT NULL CHECK (position >= 0),

    zone        INTEGER NOT NULL CHECK (zone BETWEEN 1 AND 7),
    seconds     INTEGER NOT NULL CHECK (seconds > 0),

    PRIMARY KEY (programme, microcycle, session, position),
    FOREIGN KEY (programme, microcycle, session)
        REFERENCES cycling_ride(programme, microcycle, session) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

-- The programme that answers for a date is found by date, so the index is on
-- the date. One row per name is the latest authoring; that is a `MAX` over the
-- name, as `programme` does it.
CREATE INDEX cycling_programme_start ON cycling_programme(start_date);
