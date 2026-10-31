CREATE TABLE canonical_changed_day (
    layer TEXT NOT NULL CHECK (layer IN ('gym', 'cycling', 'weigh-ins')),
    day   TEXT NOT NULL,
    PRIMARY KEY (layer, day)
) STRICT, WITHOUT ROWID;
