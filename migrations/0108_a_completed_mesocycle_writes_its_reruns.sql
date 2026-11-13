CREATE TABLE plan_completion (
    plan    INTEGER PRIMARY KEY REFERENCES plan(id) ON DELETE CASCADE,
    through TEXT    NOT NULL
) STRICT;

CREATE TABLE plan_rerun (
    plan    INTEGER NOT NULL REFERENCES plan_completion(plan) ON DELETE CASCADE,
    monday  TEXT    NOT NULL,
    holding TEXT    CHECK (holding IS NULL OR holding IN ('gym', 'cycling')),
    PRIMARY KEY (plan, monday)
) STRICT, WITHOUT ROWID;
