CREATE TABLE canonicalisation_run (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    layer        TEXT    NOT NULL CHECK (layer IN ('gym', 'cycling', 'weigh-ins')),
    started_at   TEXT    NOT NULL,
    finished_at  TEXT,
    outcome      TEXT    CHECK (outcome IN ('succeeded', 'failed')),
    read         INTEGER CHECK (read >= 0),
    written      INTEGER CHECK (written >= 0),

    CHECK ((outcome IS NULL) = (finished_at IS NULL)),
    CHECK ((outcome IS 'succeeded') = (read IS NOT NULL)),
    CHECK ((read IS NULL) = (written IS NULL))
) STRICT;
