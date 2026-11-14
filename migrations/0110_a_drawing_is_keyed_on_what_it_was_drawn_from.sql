CREATE TABLE node_revision (
    node     TEXT PRIMARY KEY CHECK (node <> ''),
    revision INTEGER NOT NULL CHECK (revision > 0)
) STRICT, WITHOUT ROWID;

ALTER TABLE kept_drawing ADD COLUMN drawn_from BLOB CHECK (drawn_from IS NULL OR length(drawn_from) = 32);
