CREATE TABLE canonical_gym_said_right (
    exercise INTEGER NOT NULL REFERENCES canonical_gym_exercise_identity(id),
    kind     TEXT    NOT NULL
        CHECK (kind IN ('dumbbell-load', 'halved-or-doubled', 'switched')),
    said_at  TEXT    NOT NULL,
    PRIMARY KEY (exercise, kind)
) STRICT, WITHOUT ROWID;
