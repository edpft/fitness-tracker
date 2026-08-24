-- The Bulgarian split squat gets its implement, and the barbell variant gets a
-- name of its own.
--
--   bulgarian-split-squat -> bulgarian-split-squat-dumbbell
--
-- `bulgarian-split-squat` was declared `Bodyweight`, and the Hevy template it
-- reads from -- `B5D3A742` -- is the dumbbell one. So the implement was wrong,
-- and § 14's loading increment was being taken from the wrong equipment: a
-- dumbbell rack does not move in a barbell's plates.
--
-- Naming it settles both. `bulgarian-split-squat-barbell` is a new member with
-- its own Hevy template (`0F24286A`) and nothing has ever been stored under it,
-- so it needs no row here -- the same case as `neutral-grip-pull-up` in `0011`.
-- The pair now reads like every other pair the vocabulary holds, which is the
-- rule that already keeps a barbell and a dumbbell preacher curl apart.
--
-- **This renames a key, not a movement.** Every set rewritten below was
-- performed with dumbbells and is still the set it was; what changes is that
-- the record now says so. The four zero-load sets among the 21 are the movement
-- done unloaded, which an absolute reading has always allowed and still does.

UPDATE performed_exercise
SET exercise = 'bulgarian-split-squat-dumbbell'
WHERE exercise = 'bulgarian-split-squat';

-- The refusal log names our exercise where it knew one, so it carries the same
-- key and takes the same rewrite.
UPDATE normalisation_refusal
SET exercise = 'bulgarian-split-squat-dumbbell'
WHERE exercise = 'bulgarian-split-squat';

UPDATE prescribed_exercise
SET exercise = 'bulgarian-split-squat-dumbbell'
WHERE exercise = 'bulgarian-split-squat';

UPDATE programme_slot_fill
SET exercise = 'bulgarian-split-squat-dumbbell'
WHERE exercise = 'bulgarian-split-squat';

UPDATE programme
SET primary_exercise = 'bulgarian-split-squat-dumbbell'
WHERE primary_exercise = 'bulgarian-split-squat';
