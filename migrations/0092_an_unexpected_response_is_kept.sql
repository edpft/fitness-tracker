CREATE TABLE unexpected_response (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    method      TEXT    NOT NULL CHECK (method <> ''),
    url         TEXT    NOT NULL CHECK (url <> ''),
    status      TEXT    NOT NULL CHECK (status <> ''),
    succeeded   INTEGER NOT NULL CHECK (succeeded IN (0, 1)),
    headers     BLOB    NOT NULL,
    body        BLOB    NOT NULL,
    detail      TEXT    NOT NULL CHECK (detail <> ''),
    answered_at TEXT    NOT NULL
) STRICT;

CREATE TRIGGER unexpected_response_is_append_only_update
BEFORE UPDATE ON unexpected_response
BEGIN
    SELECT RAISE(ABORT, 'a response is kept as it was answered');
END;

CREATE TRIGGER unexpected_response_is_append_only_delete
BEFORE DELETE ON unexpected_response
BEGIN
    SELECT RAISE(ABORT, 'a response is kept as it was answered');
END;
