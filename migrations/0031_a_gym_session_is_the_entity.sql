-- The normalised entity is the session, and a Hevy workout is part of one.
--
-- 0030 did this for cycling. Constitution 3.2.0 asks for it of every source, and
-- the operator is why: *"on the gym side, there was a time when I used different
-- Hevy routines to programme different parts of my workout so I could compose
-- them. they were all still part of the same gym session."*
--
-- 21 of his 140 training days landed more than one record, three of them four.
-- Until now each was its own entity, so every session count, frequency figure
-- and streak over those days was inflated (§ 10) and `compare` refused them as
-- an ambiguous day.
--
-- The workout table stays — a Hevy workout is real, and every column on it is
-- still the workout's — and gains the session it belongs to.

-- One session, as it was performed.
--
-- **Keyed on its first workout's landing record**, as `cycling_session` is and
-- for the same reason: a session is ours rather than the source's. Hevy names
-- each workout and names no session, so there is no source identifier to key on,
-- and the first workout's landing record is stable, unique to the session, and
-- already the thing a refusal points at.
--
-- **No `kind` column, where `cycling_session` has one.** Peloton states a
-- class's kind, so a cycling session has two shapes and a discriminant to tell
-- them apart. Hevy states nothing of the sort — nine keys in the payload, none
-- naming a kind — and the operator was asked directly: *"there are no roles,
-- they are single gym sessions split across multiple Hevy routines."* A column
-- every row answered the same way would be a shape invented here.
--
-- Nothing here is derivable from the workouts below it. When it started and how
-- long it lasted are both functions of them, and § 5 says a derived value is
-- computed rather than stored.
CREATE TABLE gym_session (
    landing_record_id  INTEGER PRIMARY KEY REFERENCES hevy_workout_landing(id),
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

-- A workout now belongs to a session.
--
-- Nullable only because SQLite cannot add a `NOT NULL` column without a default
-- to a table that has rows. Every row the derivation writes sets it, and the
-- derivation replaces this table wholesale on every run — so a null here is a
-- row written before this migration, and the next `fitness normalise
-- hevy.workouts` removes it.
ALTER TABLE gym_workout ADD COLUMN session INTEGER REFERENCES gym_session(landing_record_id);

CREATE INDEX gym_workout_by_session_composed ON gym_workout (session);
