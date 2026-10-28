CREATE TABLE kept_drawing (
    address TEXT NOT NULL PRIMARY KEY CHECK (address <> ''),
    drawing TEXT NOT NULL
) STRICT;
