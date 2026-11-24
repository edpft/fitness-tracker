ALTER TABLE padel_match RENAME TO padel_session;

ALTER TABLE padel_match_heart_rate RENAME TO padel_session_heart_rate;

DROP INDEX padel_match_by_start;

CREATE INDEX padel_session_by_start ON padel_session (started_at_utc);

CREATE TABLE padel_session_edit (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    session          TEXT    NOT NULL,
    asserted_at      TEXT    NOT NULL,
    reason           TEXT    NOT NULL CHECK (reason <> ''),
    started_at_utc   TEXT    NOT NULL,
    zone             TEXT    NOT NULL,
    duration_seconds INTEGER NOT NULL CHECK (duration_seconds > 0)
) STRICT;

CREATE INDEX padel_session_edit_by_session ON padel_session_edit (session);
