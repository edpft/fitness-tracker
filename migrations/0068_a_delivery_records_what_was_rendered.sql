ALTER TABLE prescription_delivery ADD COLUMN rendering BLOB
    CHECK (rendering IS NULL OR length(rendering) = 32);
