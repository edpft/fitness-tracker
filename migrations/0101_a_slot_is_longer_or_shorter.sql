CREATE TABLE training_slot_length (
    pattern INTEGER NOT NULL REFERENCES training_pattern(id) ON DELETE CASCADE,
    weekday TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    part    TEXT    NOT NULL
        CHECK (part IN ('morning', 'afternoon', 'evening')),
    length  TEXT    NOT NULL CHECK (length IN ('shorter', 'longer')),

    PRIMARY KEY (pattern, weekday, part)
) STRICT, WITHOUT ROWID;

INSERT INTO training_slot_length (pattern, weekday, part, length)
SELECT pattern, weekday, part,
       CASE WHEN discipline = 'cycling' AND volume = 'higher' THEN 'longer' ELSE 'shorter' END
FROM training_slot;

DROP TABLE training_slot;

ALTER TABLE training_slot_length RENAME TO training_slot;

CREATE TABLE alteration_slot_length (
    alteration INTEGER NOT NULL REFERENCES alteration(id) ON DELETE CASCADE,
    weekday    TEXT    NOT NULL
        CHECK (weekday IN ('monday', 'tuesday', 'wednesday', 'thursday',
                           'friday', 'saturday', 'sunday')),
    part       TEXT    NOT NULL
        CHECK (part IN ('morning', 'afternoon', 'evening')),
    length     TEXT    NOT NULL CHECK (length IN ('shorter', 'longer')),

    PRIMARY KEY (alteration, weekday, part)
) STRICT, WITHOUT ROWID;

INSERT INTO alteration_slot_length (alteration, weekday, part, length)
SELECT alteration, weekday, part,
       CASE WHEN discipline = 'cycling' AND volume = 'higher' THEN 'longer' ELSE 'shorter' END
FROM alteration_slot;

DROP TABLE alteration_slot;

ALTER TABLE alteration_slot_length RENAME TO alteration_slot;
