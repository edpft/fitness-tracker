CREATE TABLE body_scan_weigh_in (
    landing_record_id            INTEGER PRIMARY KEY REFERENCES withings_measurement_landing(id),
    measured_at_utc              TEXT    NOT NULL,
    zone                         TEXT    NOT NULL,
    mass_grams                   INTEGER NOT NULL CHECK (mass_grams > 0),
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

CREATE INDEX body_scan_weigh_in_by_time ON body_scan_weigh_in (measured_at_utc);

CREATE TABLE body_scan_segment (
    weigh_in             INTEGER NOT NULL REFERENCES body_scan_weigh_in(landing_record_id),
    segment              TEXT    NOT NULL
        CHECK (segment IN ('left-arm', 'right-arm', 'left-leg', 'right-leg', 'torso')),
    fat_free_mass_grams  INTEGER NOT NULL CHECK (fat_free_mass_grams >= 0),
    fat_mass_grams       INTEGER NOT NULL CHECK (fat_mass_grams >= 0),
    muscle_mass_grams    INTEGER NOT NULL CHECK (muscle_mass_grams >= 0),
    PRIMARY KEY (weigh_in, segment)
) STRICT, WITHOUT ROWID;

CREATE TABLE body_scan_heart (
    weigh_in          INTEGER PRIMARY KEY REFERENCES body_scan_weigh_in(landing_record_id),
    beats_per_minute  INTEGER NOT NULL CHECK (beats_per_minute > 0),
    rhythm            TEXT
        CHECK (rhythm IS NULL OR rhythm IN ('sinus-rhythm', 'high-heart-rate', 'not-classified'))
) STRICT;

CREATE TABLE body_scan_nerves (
    weigh_in                  INTEGER PRIMARY KEY REFERENCES body_scan_weigh_in(landing_record_id),
    left_foot_nanosiemens     INTEGER NOT NULL CHECK (left_foot_nanosiemens >= 0),
    right_foot_nanosiemens    INTEGER NOT NULL CHECK (right_foot_nanosiemens >= 0),
    both_feet_nanosiemens     INTEGER NOT NULL CHECK (both_feet_nanosiemens >= 0)
) STRICT;

CREATE TABLE body_scan_vascular (
    weigh_in                                  INTEGER PRIMARY KEY REFERENCES body_scan_weigh_in(landing_record_id),
    pulse_wave_velocity_millimetres_per_second INTEGER NOT NULL
        CHECK (pulse_wave_velocity_millimetres_per_second >= 0),
    vascular_age_tenths                       INTEGER NOT NULL CHECK (vascular_age_tenths >= 0)
) STRICT;

CREATE TABLE body_scan_part (
    landing_record_id  INTEGER NOT NULL REFERENCES withings_measurement_landing(id),
    weigh_in           INTEGER NOT NULL REFERENCES body_scan_weigh_in(landing_record_id),
    part               TEXT    NOT NULL
        CHECK (part IN ('composition', 'heart', 'nerves', 'vascular')),
    source_record_id   TEXT    NOT NULL,
    algorithm          INTEGER NOT NULL CHECK (algorithm >= 0),
    endpoint           TEXT    NOT NULL,
    event_kind         TEXT    NOT NULL,
    event_time         TEXT,
    PRIMARY KEY (landing_record_id, part)
) STRICT;

CREATE INDEX body_scan_part_by_weigh_in ON body_scan_part (weigh_in);
