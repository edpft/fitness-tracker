CREATE TABLE canonical_gym_session (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at_utc  TEXT,
    zone            TEXT,
    on_day          TEXT,
    CHECK ((on_day IS NULL) = (started_at_utc IS NOT NULL)),
    CHECK ((started_at_utc IS NULL) = (zone IS NULL))
) STRICT;

CREATE TABLE canonical_gym_session_part (
    session     INTEGER NOT NULL REFERENCES canonical_gym_session(id),
    position    INTEGER NOT NULL,
    part        TEXT    NOT NULL CHECK (part IN ('exercises', 'heart rate')),
    normalised  INTEGER NOT NULL REFERENCES gym_session(id),
    PRIMARY KEY (session, position)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX canonical_gym_session_part_is_named_once
    ON canonical_gym_session_part (session, part, normalised);

CREATE UNIQUE INDEX canonical_gym_session_part_belongs_to_one_session
    ON canonical_gym_session_part (normalised, part);
