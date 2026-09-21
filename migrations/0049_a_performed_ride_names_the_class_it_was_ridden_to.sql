ALTER TABLE bike_plus_ride ADD COLUMN venue_reference TEXT
    CHECK (venue_reference IS NULL OR length(trim(venue_reference)) > 0);

ALTER TABLE bike_plus_ride ADD COLUMN venue_called TEXT
    CHECK (venue_called IS NULL OR length(trim(venue_called)) > 0);

CREATE INDEX bike_plus_ride_by_venue ON bike_plus_ride (venue_reference);
