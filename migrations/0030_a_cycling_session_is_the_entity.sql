-- The normalised entity is the session, and a Bike+ ride is part of one.
--
-- 0029, two commits ago, made a ride the entity. Constitution 3.2.0 makes the
-- session the entity, and the operator is why: *"when we prescribe a cycling
-- session, we prescribe a main ride and a cool down ride because, from our
-- perspective, they're the same thing. This is most evident with the FTP warm
-- up and FTP test, we would never consider these to be two separate things that
-- could be planned separately but Peloton does split them."*
--
-- So splitting a session into rides is Peloton's architecture showing through,
-- exactly as splitting a ride's summary from its sample streams is. The ride
-- table stays — a ride is real, and every column on it is still the ride's —
-- and gains the session it belongs to and the part it played in it.

-- One session, as it was ridden.
--
-- **Keyed on its first ride's landing record.** A session is ours rather than
-- the source's: Peloton names each ride and names no session, so there is no
-- source identifier to key on. The first ride's landing record is stable,
-- unique to the session, and already the thing a refusal points at.
--
-- Nothing here is derivable from the rides below it. When it started, how long
-- it lasted and how far it went are all functions of them, and § 5 says a
-- derived value is computed rather than stored.
CREATE TABLE cycling_session (
    landing_record_id  INTEGER PRIMARY KEY REFERENCES peloton_workout_landing(id),
    -- Which variant. A test must have a warm-up and a ride need not, so the two
    -- are different shapes rather than one shape with a nullable column, and
    -- this is the discriminant rather than something to infer from the parts.
    kind               TEXT    NOT NULL CHECK (kind IN ('ride', 'test')),
    run_id             INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

-- A ride now belongs to a session and knows what it was for.
--
-- `role` is positional as well as declared: a low-impact class is an ordinary
-- ride on its own and a cool-down when a session ends with one, which is the
-- operator's reading of the FTP test he finished with one rather than with a
-- cool-down class.
ALTER TABLE bike_plus_ride ADD COLUMN session INTEGER REFERENCES cycling_session(landing_record_id);
ALTER TABLE bike_plus_ride ADD COLUMN role TEXT
    CHECK (role IN ('warm-up', 'effort', 'main', 'cool-down'));

CREATE INDEX bike_plus_ride_by_session ON bike_plus_ride (session);

-- A run says how many records its entities were composed from.
--
-- Needed because `workouts_written` now counts *entities* where every other
-- number counts records, and one cycling session is written from up to six of
-- them. Without this the reconciliation that § 38 rests on — every record read
-- had exactly one outcome — would compare 149 sessions against 477 records and
-- fail on every healthy run. Grouping records into sessions is exactly where a
-- record could go missing unnoticed, so the check is worth keeping.
--
-- Nullable, and null on every run recorded before now. A run that predates the
-- column cannot answer the question, and inventing a number for it would make
-- an unreconciled past run look reconciled.
ALTER TABLE normalisation_run ADD COLUMN records_composed INTEGER;

-- Peloton's nouns stop being ours.
--
-- The operator, 2026-09-08: *"on peloton.workouts and peloton.workout_samples,
-- we're mixing Peloton and our naming vocabulary here. workouts was right for
-- Hevy but it isn't the right term for Peloton and workout_samples is the thing
-- that proves it, their name suffixed to ours."*
--
-- `workout` was right for Hevy because one Hevy record was one workout. It is
-- wrong here because one Peloton record is a *part* — a ride — and the entity
-- built from them is a session. So the landing tables take our word for what
-- they hold, and `workout_samples` stops being their noun with ours bolted on.
--
-- Triggers are dropped and recreated rather than left to follow the rename.
-- SQLite does carry references across `ALTER TABLE ... RENAME`, but a raw
-- landing table's append-only guard is the one thing in this file that must not
-- depend on that being true of whichever version runs it.

DROP TRIGGER peloton_workout_landing_is_append_only_update;
DROP TRIGGER peloton_workout_landing_is_append_only_delete;
DROP TRIGGER peloton_workout_sample_landing_is_append_only_update;
DROP TRIGGER peloton_workout_sample_landing_is_append_only_delete;

ALTER TABLE peloton_workout_landing RENAME TO peloton_ride_landing;
ALTER TABLE peloton_workout_sample_landing RENAME TO peloton_ride_sample_landing;

CREATE TRIGGER peloton_ride_landing_is_append_only_update
BEFORE UPDATE ON peloton_ride_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER peloton_ride_landing_is_append_only_delete
BEFORE DELETE ON peloton_ride_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER peloton_ride_sample_landing_is_append_only_update
BEFORE UPDATE ON peloton_ride_sample_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER peloton_ride_sample_landing_is_append_only_delete
BEFORE DELETE ON peloton_ride_sample_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

-- The stream name travels as data as well as in code: a resumption point, a run
-- and a refusal are all filed under it. Renaming the table without these would
-- restart every walk from the beginning of history.
UPDATE resumption_point   SET stream = 'peloton.rides'        WHERE stream = 'peloton.workouts';
UPDATE resumption_point   SET stream = 'peloton.ride_samples' WHERE stream = 'peloton.workout_samples';
UPDATE extraction_run     SET stream = 'peloton.rides'        WHERE stream = 'peloton.workouts';
UPDATE extraction_run     SET stream = 'peloton.ride_samples' WHERE stream = 'peloton.workout_samples';
UPDATE normalisation_run  SET stream = 'peloton.rides'        WHERE stream = 'peloton.workouts';
UPDATE normalisation_run  SET stream = 'peloton.ride_samples' WHERE stream = 'peloton.workout_samples';
UPDATE normalisation_refusal SET stream = 'peloton.rides'     WHERE stream = 'peloton.workouts';

-- A run says how many records a later serving replaced.
--
-- § 10 has always said the later of two servings supersedes, and until an
-- entity composed several records it never had to be counted: one record made
-- one workout, and both stood. A session cannot do that — two servings of one
-- ride are one ride told twice, not a session of two rides — so the earlier one
-- is set aside, and a record set aside without being counted is a record with
-- no outcome.
--
-- The operator's store holds 51 of them, every one a workout landed twice
-- before the revision digest learned to ignore a class's public counters. They
-- were invisible until now.
ALTER TABLE normalisation_run ADD COLUMN records_superseded INTEGER;
