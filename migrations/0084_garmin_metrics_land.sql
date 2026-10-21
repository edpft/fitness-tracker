CREATE TABLE garmin_sleep_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_sleep_landing_latest
    ON garmin_sleep_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_sleep_landing_is_append_only_update
BEFORE UPDATE ON garmin_sleep_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_sleep_landing_is_append_only_delete
BEFORE DELETE ON garmin_sleep_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_stress_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_stress_landing_latest
    ON garmin_stress_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_stress_landing_is_append_only_update
BEFORE UPDATE ON garmin_stress_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_stress_landing_is_append_only_delete
BEFORE DELETE ON garmin_stress_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_body_battery_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_body_battery_landing_latest
    ON garmin_body_battery_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_body_battery_landing_is_append_only_update
BEFORE UPDATE ON garmin_body_battery_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_body_battery_landing_is_append_only_delete
BEFORE DELETE ON garmin_body_battery_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_body_battery_event_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_body_battery_event_landing_latest
    ON garmin_body_battery_event_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_body_battery_event_landing_is_append_only_update
BEFORE UPDATE ON garmin_body_battery_event_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_body_battery_event_landing_is_append_only_delete
BEFORE DELETE ON garmin_body_battery_event_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_respiration_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_respiration_landing_latest
    ON garmin_respiration_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_respiration_landing_is_append_only_update
BEFORE UPDATE ON garmin_respiration_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_respiration_landing_is_append_only_delete
BEFORE DELETE ON garmin_respiration_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_spo2_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_spo2_landing_latest
    ON garmin_spo2_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_spo2_landing_is_append_only_update
BEFORE UPDATE ON garmin_spo2_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_spo2_landing_is_append_only_delete
BEFORE DELETE ON garmin_spo2_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_intensity_minutes_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_intensity_minutes_landing_latest
    ON garmin_intensity_minutes_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_intensity_minutes_landing_is_append_only_update
BEFORE UPDATE ON garmin_intensity_minutes_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_intensity_minutes_landing_is_append_only_delete
BEFORE DELETE ON garmin_intensity_minutes_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_floors_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_floors_landing_latest
    ON garmin_floors_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_floors_landing_is_append_only_update
BEFORE UPDATE ON garmin_floors_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_floors_landing_is_append_only_delete
BEFORE DELETE ON garmin_floors_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_daily_summary_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_daily_summary_landing_latest
    ON garmin_daily_summary_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_daily_summary_landing_is_append_only_update
BEFORE UPDATE ON garmin_daily_summary_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_daily_summary_landing_is_append_only_delete
BEFORE DELETE ON garmin_daily_summary_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_steps_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_steps_landing_latest
    ON garmin_steps_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_steps_landing_is_append_only_update
BEFORE UPDATE ON garmin_steps_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_steps_landing_is_append_only_delete
BEFORE DELETE ON garmin_steps_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_detected_move_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_detected_move_landing_latest
    ON garmin_detected_move_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_detected_move_landing_is_append_only_update
BEFORE UPDATE ON garmin_detected_move_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_detected_move_landing_is_append_only_delete
BEFORE DELETE ON garmin_detected_move_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_vo2_max_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_vo2_max_landing_latest
    ON garmin_vo2_max_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_vo2_max_landing_is_append_only_update
BEFORE UPDATE ON garmin_vo2_max_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_vo2_max_landing_is_append_only_delete
BEFORE DELETE ON garmin_vo2_max_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_health_snapshot_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_health_snapshot_landing_latest
    ON garmin_health_snapshot_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_health_snapshot_landing_is_append_only_update
BEFORE UPDATE ON garmin_health_snapshot_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_health_snapshot_landing_is_append_only_delete
BEFORE DELETE ON garmin_health_snapshot_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_fitness_age_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_fitness_age_landing_latest
    ON garmin_fitness_age_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_fitness_age_landing_is_append_only_update
BEFORE UPDATE ON garmin_fitness_age_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_fitness_age_landing_is_append_only_delete
BEFORE DELETE ON garmin_fitness_age_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_race_prediction_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_race_prediction_landing_latest
    ON garmin_race_prediction_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_race_prediction_landing_is_append_only_update
BEFORE UPDATE ON garmin_race_prediction_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_race_prediction_landing_is_append_only_delete
BEFORE DELETE ON garmin_race_prediction_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TABLE garmin_personal_record_landing (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    endpoint          TEXT    NOT NULL,
    fetched_at        TEXT    NOT NULL,
    source_record_id  TEXT    NOT NULL,
    event_kind        TEXT    NOT NULL,
    event_time        TEXT,
    payload           BLOB    NOT NULL,
    payload_digest    BLOB    NOT NULL,
    revision_digest   BLOB,
    run_id            INTEGER NOT NULL REFERENCES extraction_run(id),
    serve_ordinal     INTEGER NOT NULL
) STRICT;

CREATE INDEX garmin_personal_record_landing_latest
    ON garmin_personal_record_landing (source_record_id, id DESC);

CREATE TRIGGER garmin_personal_record_landing_is_append_only_update
BEFORE UPDATE ON garmin_personal_record_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;

CREATE TRIGGER garmin_personal_record_landing_is_append_only_delete
BEFORE DELETE ON garmin_personal_record_landing
BEGIN
    SELECT RAISE(ABORT, 'raw landing is append-only (constitution II.1)');
END;
