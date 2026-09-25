ALTER TABLE plan ADD COLUMN chain TEXT
    CHECK (chain IS NULL OR chain IN ('base-then-build', 'build-then-peak'));

UPDATE plan SET chain = 'build-then-peak'
WHERE EXISTS (
    SELECT 1 FROM cycling_mesocycle AS c
    WHERE c.plan = plan.id AND c.provided_programme = 'Peak Your Power Zones'
);

UPDATE plan SET chain = 'base-then-build'
WHERE chain IS NULL AND EXISTS (
    SELECT 1 FROM cycling_mesocycle AS c
    WHERE c.plan = plan.id AND c.provided_programme = 'Boost Your Base'
);

DELETE FROM gym_mesocycle
WHERE ordinal > 1
  AND id NOT IN (SELECT mesocycle FROM prescribed_workout);

DELETE FROM cycling_mesocycle
WHERE ordinal > 1;
