CREATE TABLE node_revision (
    node     TEXT PRIMARY KEY CHECK (node <> ''),
    revision INTEGER NOT NULL CHECK (revision > 0)
) STRICT, WITHOUT ROWID;

ALTER TABLE kept_drawing ADD COLUMN drawn_from BLOB CHECK (drawn_from IS NULL OR length(drawn_from) = 32);

CREATE TABLE calendar_asked (
    name     TEXT PRIMARY KEY REFERENCES calendar (name),
    asked_at TEXT NOT NULL
) STRICT, WITHOUT ROWID;

INSERT INTO calendar_asked (name, asked_at) SELECT name, asked_at FROM calendar;

ALTER TABLE calendar DROP COLUMN asked_at;

DELETE FROM kept_drawing;
