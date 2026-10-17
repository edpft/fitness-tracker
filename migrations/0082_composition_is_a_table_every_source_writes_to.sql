PRAGMA defer_foreign_keys = ON;

CREATE TABLE body_composition (
    weigh_in            INTEGER NOT NULL REFERENCES weigh_in(id),
    component           TEXT    NOT NULL CHECK (component IN (
        'fat', 'fat-free', 'muscle', 'skeletal-muscle', 'lean', 'water',
        'extracellular-water', 'intracellular-water', 'bone', 'protein', 'minerals'
    )),
    mass_grams          INTEGER CHECK (mass_grams >= 0),
    volume_millilitres  INTEGER CHECK (volume_millilitres >= 0),
    percentage_tenths   INTEGER CHECK (percentage_tenths BETWEEN 0 AND 1000),
    run_id              INTEGER NOT NULL REFERENCES normalisation_run(id),
    CHECK ((mass_grams IS NOT NULL) + (volume_millilitres IS NOT NULL)
        + (percentage_tenths IS NOT NULL) = 1),
    PRIMARY KEY (weigh_in, component)
) STRICT, WITHOUT ROWID;

CREATE TABLE segment_composition (
    weigh_in            INTEGER NOT NULL REFERENCES weigh_in(id),
    segment             TEXT    NOT NULL
        CHECK (segment IN ('left-arm', 'right-arm', 'left-leg', 'right-leg', 'torso')),
    component           TEXT    NOT NULL CHECK (component IN (
        'fat', 'fat-free', 'muscle', 'skeletal-muscle', 'lean', 'water',
        'extracellular-water', 'intracellular-water', 'bone', 'protein', 'minerals'
    )),
    mass_grams          INTEGER CHECK (mass_grams >= 0),
    volume_millilitres  INTEGER CHECK (volume_millilitres >= 0),
    percentage_tenths   INTEGER CHECK (percentage_tenths BETWEEN 0 AND 1000),
    run_id              INTEGER NOT NULL REFERENCES normalisation_run(id),
    CHECK ((mass_grams IS NOT NULL) + (volume_millilitres IS NOT NULL)
        + (percentage_tenths IS NOT NULL) = 1),
    PRIMARY KEY (weigh_in, segment, component)
) STRICT, WITHOUT ROWID;

CREATE TABLE manual_composition (
    weigh_in           INTEGER NOT NULL,
    component          TEXT    NOT NULL,
    landing_record_id  INTEGER NOT NULL REFERENCES spreadsheet_file_landing(id),
    sheet              TEXT    NOT NULL CHECK (length(sheet) > 0),
    cell               TEXT    NOT NULL CHECK (length(cell) > 0),
    PRIMARY KEY (weigh_in, component),
    FOREIGN KEY (weigh_in, component) REFERENCES body_composition(weigh_in, component),
    UNIQUE (landing_record_id, sheet, cell)
) STRICT;

INSERT INTO body_composition (weigh_in, component, mass_grams, run_id)
SELECT weigh_in, 'fat-free', fat_free_mass_grams, run_id FROM body_scan_weigh_in
UNION ALL
SELECT weigh_in, 'fat', fat_mass_grams, run_id FROM body_scan_weigh_in
UNION ALL
SELECT weigh_in, 'muscle', muscle_mass_grams, run_id FROM body_scan_weigh_in
UNION ALL
SELECT weigh_in, 'water', body_water_grams, run_id FROM body_scan_weigh_in
UNION ALL
SELECT weigh_in, 'extracellular-water', extracellular_water_grams, run_id FROM body_scan_weigh_in
UNION ALL
SELECT weigh_in, 'intracellular-water', intracellular_water_grams, run_id FROM body_scan_weigh_in
UNION ALL
SELECT weigh_in, 'bone', bone_mass_grams, run_id FROM body_scan_weigh_in;

INSERT INTO segment_composition (weigh_in, segment, component, mass_grams, run_id)
SELECT b.weigh_in, s.segment, 'fat-free', s.fat_free_mass_grams, b.run_id
FROM body_scan_segment s JOIN body_scan_weigh_in b ON b.landing_record_id = s.weigh_in
UNION ALL
SELECT b.weigh_in, s.segment, 'fat', s.fat_mass_grams, b.run_id
FROM body_scan_segment s JOIN body_scan_weigh_in b ON b.landing_record_id = s.weigh_in
UNION ALL
SELECT b.weigh_in, s.segment, 'muscle', s.muscle_mass_grams, b.run_id
FROM body_scan_segment s JOIN body_scan_weigh_in b ON b.landing_record_id = s.weigh_in;

DROP TABLE body_scan_segment;

CREATE TABLE body_scan_weigh_in_staged AS SELECT * FROM body_scan_weigh_in;

DROP TABLE body_scan_weigh_in;

CREATE TABLE body_scan_weigh_in (
    landing_record_id            INTEGER PRIMARY KEY REFERENCES withings_measurement_landing(id),
    weigh_in                     INTEGER NOT NULL UNIQUE REFERENCES weigh_in(id),
    visceral_fat_tenths          INTEGER NOT NULL CHECK (visceral_fat_tenths >= 0),
    basal_metabolic_rate_kcal    INTEGER NOT NULL CHECK (basal_metabolic_rate_kcal >= 0),
    metabolic_age_tenths         INTEGER NOT NULL CHECK (metabolic_age_tenths >= 0),
    run_id                       INTEGER NOT NULL REFERENCES normalisation_run(id)
) STRICT;

INSERT INTO body_scan_weigh_in (
    landing_record_id, weigh_in, visceral_fat_tenths, basal_metabolic_rate_kcal,
    metabolic_age_tenths, run_id
)
SELECT landing_record_id, weigh_in, visceral_fat_tenths, basal_metabolic_rate_kcal,
       metabolic_age_tenths, run_id
FROM body_scan_weigh_in_staged;

DROP TABLE body_scan_weigh_in_staged;
