ALTER TABLE performed_set ADD COLUMN landing_record_id INTEGER;

UPDATE performed_set
SET landing_record_id = (
    SELECT w.landing_record_id FROM gym_workout AS w WHERE w.id = performed_set.workout
)
WHERE sheet IS NOT NULL;
