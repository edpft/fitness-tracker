-- A prescribed workout is drafted, published, or performed.
--
-- **The state is derived, never stored.** A status column would be a second
-- source of truth for a fact the relations already carry, and the two could
-- disagree:
--
--   drafted    a `prescribed_workout` with no `prescription_delivery` row.
--              Nothing outside this store knows it exists.
--   published  delivered somewhere it can be performed, and fixed by the
--              reference that destination gave it. Still cheap.
--   performed  a `gym_workout` names that reference. Now it is not.
--
-- **Which one it is decides what may be done to it.** A drafted prescription
-- re-derives exactly from its programme, the record and the parameters, so
-- nothing is lost by deleting it. A published one is the same, plus a routine
-- at the destination that has to be withdrawn rather than forgotten -- leaving
-- it behind is what cluttered the source badly enough that the operator deleted
-- his routines, which is how the record came to hold 155 workouts naming no
-- routine at all. A performed one is pinned by an observation, and § 12 applies
-- to it in full.
--
-- This is the answer to the question § 12 left open. It says authored data
-- keeps its history because "nothing regenerates it if lost", and that premise
-- is false for a prescription nobody has performed. It is not false for one
-- somebody has.

-- The session a workout was performed against, as the destination named it.
--
-- Null is the ordinary case for everything already in the record: a session
-- logged freehand, or against a routine since deleted. A real state, not a gap.
ALTER TABLE gym_workout ADD COLUMN performed_against TEXT;

CREATE INDEX gym_workout_by_session ON gym_workout (performed_against)
    WHERE performed_against IS NOT NULL;

-- **A performed prescription cannot be deleted.** The same treatment raw
-- landing gets, and for the same reason: what it records happened, and the
-- performance beside it would be left comparing against nothing.
--
-- A trigger rather than a rule in code, so it holds against every writer that
-- ever exists -- including a hand-typed `DELETE` at a prompt.
CREATE TRIGGER prescribed_workout_performed_is_not_deletable
BEFORE DELETE ON prescribed_workout
WHEN EXISTS (
    SELECT 1
    FROM prescription_delivery AS d
    JOIN gym_workout AS w ON w.performed_against = d.reference
    WHERE d.prescription = OLD.id
)
BEGIN
    SELECT RAISE(ABORT, 'a performed prescription is not deletable (constitution 12)');
END;

-- Withdrawing a published session drops its delivery row, so the same rule has
-- to hold there: the receipt is what the performance is joined through, and
-- removing it would make a performed prescription look drafted again.
CREATE TRIGGER prescription_delivery_performed_is_not_deletable
BEFORE DELETE ON prescription_delivery
WHEN EXISTS (
    SELECT 1 FROM gym_workout WHERE performed_against = OLD.reference
)
BEGIN
    SELECT RAISE(ABORT, 'a performed session is not withdrawable (constitution 12)');
END;
