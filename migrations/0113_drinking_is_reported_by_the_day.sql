CREATE TABLE drinking_day (
    day     TEXT PRIMARY KEY,
    authored_at TEXT NOT NULL,
    amount      TEXT NOT NULL CHECK (amount IN ('unknown', 'at_least', 'exactly'))
) STRICT, WITHOUT ROWID;

CREATE TABLE drinking_drinks (
    day        TEXT    NOT NULL REFERENCES drinking_day(day) ON DELETE CASCADE,
    position       INTEGER NOT NULL CHECK (position >= 0),
    count          INTEGER NOT NULL CHECK (count > 0 AND count <= 255),
    name           TEXT    NOT NULL CHECK (length(trim(name)) > 0),
    millilitres    INTEGER NOT NULL CHECK (millilitres > 0 AND millilitres <= 65535),
    strength       INTEGER NOT NULL CHECK (strength > 0 AND strength <= 1000),

    PRIMARY KEY (day, position)
) STRICT, WITHOUT ROWID;

CREATE TABLE own_drink (
    name        TEXT    PRIMARY KEY CHECK (length(trim(name)) > 0),
    authored_at TEXT    NOT NULL,
    millilitres INTEGER NOT NULL CHECK (millilitres > 0 AND millilitres <= 65535),
    strength    INTEGER NOT NULL CHECK (strength > 0 AND strength <= 1000)
) STRICT, WITHOUT ROWID;
