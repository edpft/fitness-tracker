ALTER TABLE normalisation_run ADD COLUMN derived_from TEXT;

CREATE TABLE canonical_derivation (
    layer         TEXT PRIMARY KEY CHECK (layer IN ('gym', 'cycling', 'weigh-ins')),
    derived_from  TEXT NOT NULL
) STRICT;

CREATE TABLE derivation_build (
    id     INTEGER PRIMARY KEY CHECK (id = 1),
    build  TEXT    NOT NULL CHECK (length(trim(build)) > 0)
) STRICT;
