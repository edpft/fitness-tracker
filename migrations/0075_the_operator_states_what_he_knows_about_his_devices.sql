CREATE TABLE device_statement (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    stated_at    TEXT    NOT NULL,
    kind         TEXT    NOT NULL
        CHECK (kind IN ('added', 'first-used', 'last-used', 'named', 'sends', 'same-device')),
    device       TEXT    CHECK ((kind = 'added') = (device IS NULL)),
    other_device TEXT    CHECK ((kind = 'same-device') = (other_device IS NOT NULL)),
    first_used   TEXT    CHECK ((kind IN ('added', 'first-used')) = (first_used IS NOT NULL)),
    last_used    TEXT    CHECK ((kind IN ('added', 'last-used')) = (last_used IS NOT NULL)),
    name         TEXT    CHECK ((kind IN ('added', 'named')) = (name IS NOT NULL)),
    measures     TEXT    CHECK ((kind IN ('added', 'sends')) = (measures IS NOT NULL))
) STRICT;
