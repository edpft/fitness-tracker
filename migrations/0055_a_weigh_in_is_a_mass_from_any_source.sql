PRAGMA defer_foreign_keys = ON;

CREATE TABLE weigh_in (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    stream           TEXT    NOT NULL,
    on_day           TEXT,
    measured_at_utc  TEXT,
    zone             TEXT,
    mass_grams       INTEGER NOT NULL CHECK (mass_grams > 0),
    run_id           INTEGER NOT NULL REFERENCES normalisation_run(id),
    CHECK ((on_day IS NULL) = (measured_at_utc IS NOT NULL)),
    CHECK ((measured_at_utc IS NULL) = (zone IS NULL))
) STRICT;

CREATE INDEX weigh_in_by_stream ON weigh_in (stream);

CREATE INDEX weigh_in_by_time ON weigh_in (measured_at_utc);

CREATE TABLE manual_weigh_in (
    weigh_in           INTEGER PRIMARY KEY REFERENCES weigh_in(id),
    landing_record_id  INTEGER NOT NULL REFERENCES spreadsheet_file_landing(id),
    sheet              TEXT    NOT NULL CHECK (length(sheet) > 0),
    cell               TEXT    NOT NULL CHECK (length(cell) > 0),
    UNIQUE (landing_record_id, sheet, cell)
) STRICT;

INSERT INTO weigh_in (id, stream, measured_at_utc, zone, mass_grams, run_id)
SELECT landing_record_id, 'withings.measurements', measured_at_utc, zone, mass_grams, run_id
FROM body_scan_weigh_in;

CREATE TABLE body_scan_weigh_in_staged AS SELECT * FROM body_scan_weigh_in;

DROP TABLE body_scan_weigh_in;

CREATE TABLE body_scan_weigh_in (
    landing_record_id            INTEGER PRIMARY KEY REFERENCES withings_measurement_landing(id),
    weigh_in                     INTEGER NOT NULL UNIQUE REFERENCES weigh_in(id),
    fat_free_mass_grams          INTEGER NOT NULL CHECK (fat_free_mass_grams >= 0),
    fat_mass_grams               INTEGER NOT NULL CHECK (fat_mass_grams >= 0),
    muscle_mass_grams            INTEGER NOT NULL CHECK (muscle_mass_grams >= 0),
    body_water_grams             INTEGER NOT NULL CHECK (body_water_grams >= 0),
    extracellular_water_grams    INTEGER NOT NULL CHECK (extracellular_water_grams >= 0),
    intracellular_water_grams    INTEGER NOT NULL CHECK (intracellular_water_grams >= 0),
    bone_mass_grams              INTEGER NOT NULL CHECK (bone_mass_grams >= 0),
    visceral_fat_tenths          INTEGER NOT NULL CHECK (visceral_fat_tenths >= 0),
    basal_metabolic_rate_kcal    INTEGER NOT NULL CHECK (basal_metabolic_rate_kcal >= 0),
    metabolic_age_tenths         INTEGER NOT NULL CHECK (metabolic_age_tenths >= 0),
    run_id                       INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

INSERT INTO body_scan_weigh_in (
    landing_record_id, weigh_in, fat_free_mass_grams, fat_mass_grams, muscle_mass_grams,
    body_water_grams, extracellular_water_grams, intracellular_water_grams, bone_mass_grams,
    visceral_fat_tenths, basal_metabolic_rate_kcal, metabolic_age_tenths, run_id
)
SELECT landing_record_id, landing_record_id, fat_free_mass_grams, fat_mass_grams,
       muscle_mass_grams, body_water_grams, extracellular_water_grams,
       intracellular_water_grams, bone_mass_grams, visceral_fat_tenths,
       basal_metabolic_rate_kcal, metabolic_age_tenths, run_id
FROM body_scan_weigh_in_staged;

DROP TABLE body_scan_weigh_in_staged;
