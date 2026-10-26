//! Every normalised table, as rows that compare between two stores (#571).
//!
//! **What a derivation wrote, not where it filed it.** Two stores derived the
//! same way hold the same rows, but a surrogate id is a counter, and a run id
//! names the run; an incremental derivation writes the sessions it revised
//! under ids a whole one would not. So each row is read with those left out and
//! a reference to a surrogate id replaced by what that row is: a workout by its
//! stream, landing record and source record, a weigh-in by its stream, day,
//! moment and mass, a sighting by its stream, record, device and role.
//!
//! Standalone, so a suite reaches it with `#[path]` without compiling the
//! corpus fixtures it has no use for.

use std::collections::BTreeMap;

use sqlx::SqlitePool;

/// The tables a surrogate id makes need a query of their own.
const KEYED: &[(&str, &str)] = &[
    (
        "gym_session",
        "SELECT json_array(stream, landing_record_id) FROM gym_session",
    ),
    (
        "gym_workout",
        "SELECT json_array(w.stream, s.landing_record_id, w.landing_record_id, \
                w.source_record_id, w.started_at_utc, w.zone, w.on_day, w.endpoint, \
                w.event_kind, w.event_time, w.performed_against, w.duration_seconds) \
           FROM gym_workout w JOIN gym_session s ON s.id = w.session",
    ),
    (
        "workout_item",
        "SELECT json_array(w.stream, w.landing_record_id, w.source_record_id, \
                w.started_at_utc, w.on_day, t.position, t.is_superset) \
           FROM workout_item t JOIN gym_workout w ON w.id = t.workout",
    ),
    (
        "performed_exercise",
        "SELECT json_array(w.stream, w.landing_record_id, w.source_record_id, \
                w.started_at_utc, w.on_day, t.item_position, t.position, t.exercise, \
                t.measure) \
           FROM performed_exercise t JOIN gym_workout w ON w.id = t.workout",
    ),
    (
        "performed_set",
        "SELECT json_array(w.stream, w.landing_record_id, w.source_record_id, \
                w.started_at_utc, w.on_day, t.item_position, t.exercise_position, \
                t.position, t.load_kind, t.load_grams, t.outcome, t.reps, \
                t.duration_seconds, t.distance_mm, t.rir, t.set_kind, \
                t.rest_after_seconds, t.sheet, t.cell, t.landing_record_id) \
           FROM performed_set t JOIN gym_workout w ON w.id = t.workout",
    ),
    (
        "measured_set",
        "SELECT json_array(w.stream, w.landing_record_id, w.source_record_id, \
                w.started_at_utc, w.on_day, t.position, t.started_at_utc, t.zone, t.reps, \
                t.load_kind, t.load_grams, t.guess_exercise, t.guess_movement, \
                t.guess_implement, t.guess_from) \
           FROM measured_set t JOIN gym_workout w ON w.id = t.workout",
    ),
    (
        "measured_gym_session",
        "SELECT json_array(w.stream, w.landing_record_id, w.source_record_id, \
                w.started_at_utc, w.on_day, t.duration_seconds, t.average_bpm, \
                t.highest_bpm, t.sets_landing_record_id, t.recording_landing_record_id) \
           FROM measured_gym_session t JOIN gym_workout w ON w.id = t.workout",
    ),
    (
        "measured_gym_session_heart_rate",
        "SELECT json_array(w.stream, w.landing_record_id, w.source_record_id, \
                w.started_at_utc, w.on_day, t.at_seconds, t.beats_per_minute) \
           FROM measured_gym_session_heart_rate t JOIN gym_workout w ON w.id = t.workout",
    ),
    (
        "weigh_in",
        "SELECT json_array(stream, on_day, measured_at_utc, zone, mass_grams) FROM weigh_in",
    ),
    (
        "body_composition",
        "SELECT json_array(w.stream, w.on_day, w.measured_at_utc, w.mass_grams, \
                t.component, t.mass_grams, t.volume_millilitres, t.percentage_tenths) \
           FROM body_composition t JOIN weigh_in w ON w.id = t.weigh_in",
    ),
    (
        "segment_composition",
        "SELECT json_array(w.stream, w.on_day, w.measured_at_utc, w.mass_grams, \
                t.segment, t.component, t.mass_grams, t.volume_millilitres, \
                t.percentage_tenths) \
           FROM segment_composition t JOIN weigh_in w ON w.id = t.weigh_in",
    ),
    (
        "manual_composition",
        "SELECT json_array(w.stream, w.on_day, w.measured_at_utc, w.mass_grams, \
                t.component, t.landing_record_id, t.sheet, t.cell) \
           FROM manual_composition t JOIN weigh_in w ON w.id = t.weigh_in",
    ),
    (
        "manual_weigh_in",
        "SELECT json_array(w.stream, w.on_day, w.measured_at_utc, w.mass_grams, \
                t.landing_record_id, t.sheet, t.cell, t.last_saved_at_utc, t.last_saved_by) \
           FROM manual_weigh_in t JOIN weigh_in w ON w.id = t.weigh_in",
    ),
    (
        "myfitnesspal_weigh_in",
        "SELECT json_array(w.stream, w.on_day, w.measured_at_utc, w.mass_grams, \
                t.landing_record_id, t.sheet, t.cell, t.entered_at_utc) \
           FROM myfitnesspal_weigh_in t JOIN weigh_in w ON w.id = t.weigh_in",
    ),
    (
        "body_scan_weigh_in",
        "SELECT json_array(w.stream, w.on_day, w.measured_at_utc, w.mass_grams, \
                t.landing_record_id, t.visceral_fat_tenths, t.basal_metabolic_rate_kcal, \
                t.metabolic_age_tenths) \
           FROM body_scan_weigh_in t JOIN weigh_in w ON w.id = t.weigh_in",
    ),
    (
        "device_sighting",
        "SELECT json_array(stream, landing_record_id, source_record_id, device, name, role, \
                worn, at_utc, zone) \
           FROM device_sighting",
    ),
    (
        "device_sighting_identifier",
        "SELECT json_array(d.stream, d.landing_record_id, d.device, d.role, d.at_utc, \
                t.position, t.identifier) \
           FROM device_sighting_identifier t JOIN device_sighting d ON d.id = t.sighting",
    ),
    (
        "device_sighting_measurand",
        "SELECT json_array(d.stream, d.landing_record_id, d.device, d.role, d.at_utc, \
                t.measurand, t.how) \
           FROM device_sighting_measurand t JOIN device_sighting d ON d.id = t.sighting",
    ),
    (
        "normalisation_refusal",
        "SELECT json_array(stream, landing_record_id, source_record_id, locus_kind, \
                entry_index, set_index, group_id, exercise, reason, kind, detail) \
           FROM normalisation_refusal",
    ),
];

/// The tables keyed on what they are rather than on a counter: every column but
/// the run.
const PLAIN: &[&str] = &[
    "cycling_session",
    "bike_plus_ride",
    "bike_plus_ride_sample",
    "bike_plus_ride_heart_rate",
    "ftp",
    "overnight_hrv",
    "overnight_hrv_reading",
    "resting_heart_rate",
    "wear_day",
    "body_scan_part",
    "body_scan_heart",
    "body_scan_nerves",
    "body_scan_vascular",
];

/// Every normalised table's rows, sorted, by table.
///
/// # Errors
///
/// [`sqlx::Error`] if a table cannot be read.
pub async fn dump(pool: &SqlitePool) -> Result<BTreeMap<String, Vec<String>>, sqlx::Error> {
    let mut tables = BTreeMap::new();
    for (table, query) in KEYED {
        let mut rows: Vec<String> = sqlx::query_scalar(*query).fetch_all(pool).await?;
        rows.sort();
        tables.insert((*table).to_owned(), rows);
    }
    for table in PLAIN {
        let columns: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM pragma_table_info(?) WHERE name <> 'run_id' ORDER BY cid",
        )
        .bind(table)
        .fetch_all(pool)
        .await?;
        let query = format!("SELECT json_array({}) FROM {table}", columns.join(", "));
        // Built from the table's own column names, read just above.
        let mut rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(query))
            .fetch_all(pool)
            .await?;
        rows.sort();
        tables.insert((*table).to_owned(), rows);
    }
    Ok(tables)
}
