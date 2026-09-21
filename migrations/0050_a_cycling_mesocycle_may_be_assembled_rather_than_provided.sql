-- no-transaction
PRAGMA foreign_keys = OFF;

CREATE TABLE cycling_mesocycle_rebuilt (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,

    plan               INTEGER NOT NULL REFERENCES plan(id) ON DELETE CASCADE,
    ordinal            INTEGER NOT NULL CHECK (ordinal > 0),

    provider           TEXT    CHECK (provider IS NULL
                                      OR length(trim(provider)) > 0),
    provided_programme TEXT    CHECK (provided_programme IS NULL
                                      OR length(trim(provided_programme)) > 0),

    start_date         TEXT    NOT NULL,

    CHECK ((provider IS NULL) = (provided_programme IS NULL)),

    UNIQUE (plan, ordinal)
) STRICT;

INSERT INTO cycling_mesocycle_rebuilt (id, plan, ordinal, provider, provided_programme, start_date)
    SELECT id, plan, ordinal, provider, provided_programme, start_date FROM cycling_mesocycle;

CREATE TABLE cycling_microcycle_rebuilt (
    mesocycle          INTEGER NOT NULL REFERENCES cycling_mesocycle_rebuilt(id) ON DELETE CASCADE,
    ordinal            INTEGER NOT NULL CHECK (ordinal > 0),

    PRIMARY KEY (mesocycle, ordinal)
) STRICT, WITHOUT ROWID;

INSERT INTO cycling_microcycle_rebuilt (mesocycle, ordinal)
    SELECT mesocycle, ordinal FROM cycling_microcycle;

CREATE TABLE cycling_ride_rebuilt (
    mesocycle         INTEGER NOT NULL REFERENCES cycling_mesocycle_rebuilt(id) ON DELETE CASCADE,
    microcycle        INTEGER NOT NULL CHECK (microcycle > 0),
    session           INTEGER NOT NULL CHECK (session > 0),

    published_microcycle INTEGER CHECK (published_microcycle IS NULL
                                        OR published_microcycle > 0),
    published_session INTEGER CHECK (published_session IS NULL
                                     OR published_session > 0),

    intensity         TEXT    NOT NULL CHECK (intensity IN ('higher', 'lower')),
    volume            TEXT    NOT NULL CHECK (volume IN ('higher', 'lower')),

    warm_up_seconds   INTEGER NOT NULL CHECK (warm_up_seconds > 0),
    cool_down_seconds INTEGER CHECK (cool_down_seconds IS NULL OR cool_down_seconds > 0),
    effort_seconds    INTEGER CHECK (effort_seconds IS NULL OR effort_seconds > 0),

    PRIMARY KEY (mesocycle, microcycle, session),
    UNIQUE (mesocycle, microcycle, intensity, volume),

    CHECK ((published_microcycle IS NULL) = (published_session IS NULL)),

    FOREIGN KEY (mesocycle, microcycle)
        REFERENCES cycling_microcycle_rebuilt(mesocycle, ordinal) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

INSERT INTO cycling_ride_rebuilt (mesocycle, microcycle, session,
                                  published_microcycle, published_session,
                                  intensity, volume, warm_up_seconds,
                                  cool_down_seconds, effort_seconds)
    SELECT r.mesocycle, r.microcycle, r.session,
           (SELECT c.published_ordinal FROM cycling_microcycle c
             WHERE c.mesocycle = r.mesocycle AND c.ordinal = r.microcycle),
           r.published_session,
           r.intensity, r.volume, r.warm_up_seconds, r.cool_down_seconds,
           r.effort_seconds
    FROM cycling_ride r;

DROP TABLE cycling_ride;
DROP TABLE cycling_microcycle;
DROP TABLE cycling_mesocycle;

ALTER TABLE cycling_mesocycle_rebuilt RENAME TO cycling_mesocycle;
ALTER TABLE cycling_microcycle_rebuilt RENAME TO cycling_microcycle;
ALTER TABLE cycling_ride_rebuilt RENAME TO cycling_ride;

CREATE INDEX cycling_mesocycle_start ON cycling_mesocycle(start_date);

PRAGMA foreign_keys = ON;
