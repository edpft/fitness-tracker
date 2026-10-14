ALTER TABLE manual_weigh_in ADD COLUMN last_saved_at_utc TEXT;

ALTER TABLE manual_weigh_in ADD COLUMN last_saved_by TEXT
    CHECK (last_saved_by IN ('document', 'file system'));

CREATE TABLE canonical_weigh_in (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    on_day             TEXT,
    measured_at_utc    TEXT,
    zone               TEXT,
    occurred_weigh_in  INTEGER NOT NULL REFERENCES weigh_in(id),
    mass_grams         INTEGER NOT NULL CHECK (mass_grams > 0),
    mass_weigh_in      INTEGER NOT NULL REFERENCES weigh_in(id),
    CHECK ((on_day IS NULL) = (measured_at_utc IS NOT NULL)),
    CHECK ((measured_at_utc IS NULL) = (zone IS NULL))
) STRICT;

CREATE TABLE canonical_weigh_in_account (
    canonical  INTEGER NOT NULL REFERENCES canonical_weigh_in(id),
    weigh_in   INTEGER NOT NULL UNIQUE REFERENCES weigh_in(id),
    PRIMARY KEY (canonical, weigh_in)
) STRICT, WITHOUT ROWID;
