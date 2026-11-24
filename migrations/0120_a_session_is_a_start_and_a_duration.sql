-- no-transaction
PRAGMA foreign_keys = OFF;

CREATE TABLE canonical_gym_session_edit_rebuilt (
    edit             INTEGER NOT NULL REFERENCES canonical_gym_edit(id) ON DELETE CASCADE,
    session          INTEGER NOT NULL REFERENCES canonical_gym_session_identity(id),
    started_from     TEXT,
    started_at_utc   TEXT,
    started_zone     TEXT,
    started_on_day   TEXT,
    ended_from       TEXT,
    duration_seconds INTEGER CHECK (duration_seconds IS NULL OR duration_seconds > 0),
    PRIMARY KEY (edit, session),
    CHECK ((started_at_utc IS NULL) = (started_zone IS NULL)),
    CHECK (started_at_utc IS NULL OR started_on_day IS NULL),
    CHECK (started_from IS NULL OR (started_at_utc IS NULL AND started_on_day IS NULL)),
    CHECK (ended_from IS NULL OR duration_seconds IS NULL),
    CHECK (started_from IS NOT NULL OR started_at_utc IS NOT NULL OR started_on_day IS NOT NULL
           OR ended_from IS NOT NULL OR duration_seconds IS NOT NULL)
) STRICT, WITHOUT ROWID;

INSERT INTO canonical_gym_session_edit_rebuilt
    (edit, session, started_from, started_at_utc, started_zone, started_on_day,
     ended_from, duration_seconds)
SELECT edit, session, started_from, started_at_utc, started_zone, started_on_day, ended_from,
       duration_seconds
FROM (
    SELECT x.*,
           CASE WHEN x.ended_at_utc IS NOT NULL THEN
               unixepoch(x.ended_at_utc) - unixepoch(coalesce(
                   x.started_at_utc,
                   (SELECT s.started_at_utc FROM canonical_gym_session AS s
                     WHERE s.id = x.session)))
           END AS duration_seconds
    FROM canonical_gym_session_edit AS x
)
WHERE (duration_seconds IS NULL OR duration_seconds > 0)
  AND (started_from IS NOT NULL OR started_at_utc IS NOT NULL OR started_on_day IS NOT NULL
       OR ended_from IS NOT NULL OR duration_seconds IS NOT NULL);

DROP TABLE canonical_gym_session_edit;
ALTER TABLE canonical_gym_session_edit_rebuilt RENAME TO canonical_gym_session_edit;

DELETE FROM canonical_gym_set;
DELETE FROM canonical_gym_exercise;
DELETE FROM canonical_gym_item;
DELETE FROM canonical_gym_session_heart_rate;
DELETE FROM canonical_gym_session_account;
DELETE FROM canonical_gym_session;
DELETE FROM canonical_changed_day WHERE layer = 'gym';
DELETE FROM canonical_derivation WHERE layer = 'gym';

DROP TABLE canonical_gym_session_account;
DROP TABLE canonical_gym_session;

CREATE TABLE canonical_gym_session (
    id                            INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at_utc                TEXT,
    zone                          TEXT,
    on_day                        TEXT,
    started_normalised_session    INTEGER REFERENCES gym_session(id),
    duration_seconds              INTEGER CHECK (duration_seconds IS NULL OR duration_seconds > 0),
    duration_normalised_session   INTEGER REFERENCES gym_session(id),
    average_bpm                   INTEGER,
    highest_bpm                   INTEGER,
    heart_rate_normalised_session INTEGER REFERENCES gym_session(id),
    CHECK ((on_day IS NULL) = (started_at_utc IS NOT NULL)),
    CHECK ((started_at_utc IS NULL) = (zone IS NULL)),
    CHECK (duration_seconds IS NULL OR started_at_utc IS NOT NULL),
    CHECK (duration_seconds IS NOT NULL OR duration_normalised_session IS NULL),
    CHECK ((average_bpm IS NULL) = (highest_bpm IS NULL)),
    CHECK ((average_bpm IS NULL) = (heart_rate_normalised_session IS NULL)),
    CHECK (average_bpm IS NULL OR average_bpm > 0),
    CHECK (highest_bpm IS NULL OR highest_bpm > 0)
) STRICT;

CREATE INDEX canonical_gym_session_by_day ON canonical_gym_session (on_day)
    WHERE on_day IS NOT NULL;
CREATE INDEX canonical_gym_session_by_instant ON canonical_gym_session (started_at_utc)
    WHERE started_at_utc IS NOT NULL;

CREATE TABLE canonical_gym_session_account (
    session            INTEGER NOT NULL REFERENCES canonical_gym_session(id),
    normalised_session INTEGER NOT NULL REFERENCES gym_session(id),
    stream             TEXT    NOT NULL,
    started_at_utc     TEXT,
    zone               TEXT,
    on_day             TEXT,
    duration_seconds   INTEGER CHECK (duration_seconds IS NULL OR duration_seconds > 0),
    PRIMARY KEY (session, normalised_session),
    CHECK ((on_day IS NULL) = (started_at_utc IS NOT NULL)),
    CHECK ((started_at_utc IS NULL) = (zone IS NULL)),
    CHECK (duration_seconds IS NULL OR started_at_utc IS NOT NULL)
) STRICT, WITHOUT ROWID;

PRAGMA foreign_keys = ON;
