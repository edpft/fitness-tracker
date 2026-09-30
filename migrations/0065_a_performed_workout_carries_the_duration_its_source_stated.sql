ALTER TABLE gym_workout
    ADD COLUMN duration_seconds INTEGER
    CHECK (duration_seconds IS NULL OR duration_seconds > 0);
