//! Raw landing for Garmin's metrics beyond activities and HRV (#269).
//!
//! One table per metric, each answer as served. See [`crate::garmin::days`],
//! [`crate::garmin::months`] and [`crate::garmin::latest`] for the walks that
//! fill them.
//!
//! **Generated rather than copied.** Each table is shaped exactly as
//! [`super::garmin_daily_heart_rate_landing`]'s and answers the same four
//! queries, and the only thing that differs is a name `query!` needs as a
//! literal. The macro splices the table into each query as a string literal
//! joined with `+`, which `query!` accepts, so every query is still checked
//! against the schema at build time; sixteen hand copies would be sixteen
//! places for one of them to drift.

use application::{LandingStore, StoreError};
use domain::landing::{
    InvalidStream, LandingRecord, LandingStream, PayloadDigest, RecordCount, RunId, SourceRecordId,
};
use sqlx::SqlitePool;

use super::{count_from_storage, digest_from_row, run_id_for_storage, store_error};

macro_rules! metric_landing_store {
    ($(#[$doc:meta])* $store:ident, $stream:literal, $table:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone)]
        pub struct $store {
            pool: SqlitePool,
            stream: LandingStream,
        }

        impl $store {
            /// Which stream this table holds, declared beside the queries
            /// that name it, for the reason given on
            /// [`super::landing::HevyWorkoutLandingStore::STREAM`].
            pub const STREAM: &'static str = $stream;

            /// # Errors
            ///
            /// [`InvalidStream`] if [`Self::STREAM`] is not a stream name.
            /// Pinned by a test below, so it is a mistake in this file rather
            /// than in a call.
            pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
                Ok(Self {
                    pool,
                    stream: LandingStream::try_from(Self::STREAM)?,
                })
            }
        }

        impl LandingStore for $store {
            fn stream(&self) -> &LandingStream {
                &self.stream
            }

            async fn latest_digest(
                &self,
                id: &SourceRecordId,
            ) -> Result<Option<PayloadDigest>, StoreError> {
                let id = id.as_str();
                let row = sqlx::query!(
                    r#"SELECT COALESCE(revision_digest, payload_digest) AS "payload_digest!: Vec<u8>" FROM "#
                        + $table
                        + r#" WHERE source_record_id = ? ORDER BY id DESC LIMIT 1"#,
                    id
                )
                .fetch_optional(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;

                row.map(|row| digest_from_row(&row.payload_digest))
                    .transpose()
            }

            async fn append(
                &self,
                run: RunId,
                records: Vec<LandingRecord>,
            ) -> Result<RecordCount, StoreError> {
                if records.is_empty() {
                    return Ok(RecordCount::default());
                }

                let mut transaction = self
                    .pool
                    .begin()
                    .await
                    .map_err(|error| store_error(&error))?;

                let run_id = run_id_for_storage(run)?;
                let next = sqlx::query!(
                    r#"SELECT COALESCE(MAX(serve_ordinal), -1) AS "highest!: i64" FROM "#
                        + $table
                        + r#" WHERE run_id = ?"#,
                    run_id
                )
                .fetch_one(&mut *transaction)
                .await
                .map_err(|error| store_error(&error))?;

                let mut ordinal = next.highest;
                let mut landed = 0_usize;

                for record in &records {
                    ordinal = ordinal.saturating_add(1);

                    let event = super::served_by_a_feed(record.provenance())?;

                    let endpoint = event.endpoint().as_str();
                    let fetched_at = record.fetched_at().to_string();
                    let source_record_id = record.source_record_id().as_str();
                    let event_kind = event.kind().as_str();
                    let event_time = event.occurred_at().map(|at| at.to_string());
                    let payload = record.payload().as_bytes();
                    let digest = record.digest();
                    let digest = digest.as_bytes().as_slice();
                    let revision = record.revision();
                    let revision = revision.as_bytes().as_slice();

                    sqlx::query!(
                        "INSERT INTO "
                            + $table
                            + " (endpoint, fetched_at, source_record_id, event_kind, \
                               event_time, payload, payload_digest, revision_digest, \
                               run_id, serve_ordinal) \
                               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        endpoint,
                        fetched_at,
                        source_record_id,
                        event_kind,
                        event_time,
                        payload,
                        digest,
                        revision,
                        run_id,
                        ordinal
                    )
                    .execute(&mut *transaction)
                    .await
                    .map_err(|error| store_error(&error))?;

                    landed = landed.saturating_add(1);
                }

                transaction
                    .commit()
                    .await
                    .map_err(|error| store_error(&error))?;

                Ok(RecordCount::from(landed))
            }

            async fn count(&self) -> Result<RecordCount, StoreError> {
                let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM "# + $table)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|error| store_error(&error))?;

                Ok(RecordCount::from(count_from_storage(Some(row.total))?))
            }
        }

        impl application::RawExtent for $store {
            async fn records(&self) -> Result<RecordCount, StoreError> {
                LandingStore::count(self).await
            }
        }
    };
}

metric_landing_store!(
    /// A night's sleep: its stages, and since about 2026-05 the readings taken
    /// through it.
    GarminSleepLandingStore,
    "garmin.sleep",
    "garmin_sleep_landing"
);
metric_landing_store!(
    /// A day's stress, and its Body Battery readings.
    GarminStressLandingStore,
    "garmin.stress",
    "garmin_stress_landing"
);
metric_landing_store!(
    /// A day's Body Battery: what it charged and drained.
    GarminBodyBatteryLandingStore,
    "garmin.body_battery",
    "garmin_body_battery_landing"
);
metric_landing_store!(
    /// The events of a day that moved the Body Battery: a night, an activity.
    GarminBodyBatteryEventLandingStore,
    "garmin.body_battery_events",
    "garmin_body_battery_event_landing"
);
metric_landing_store!(
    /// A day's breathing rate.
    GarminRespirationLandingStore,
    "garmin.respiration",
    "garmin_respiration_landing"
);
metric_landing_store!(
    /// A day's blood oxygen saturation, as the Pulse Ox sensor read it.
    GarminSpo2LandingStore,
    "garmin.spo2",
    "garmin_spo2_landing"
);
metric_landing_store!(
    /// A day's moderate and vigorous minutes.
    GarminIntensityMinutesLandingStore,
    "garmin.intensity_minutes",
    "garmin_intensity_minutes_landing"
);
metric_landing_store!(
    /// A day's floors climbed and descended.
    GarminFloorsLandingStore,
    "garmin.floors",
    "garmin_floors_landing"
);
metric_landing_store!(
    /// A day's totals: steps, calories, distance, and the day's figures for
    /// most of the other metrics.
    GarminDailySummaryLandingStore,
    "garmin.daily_summary",
    "garmin_daily_summary_landing"
);
metric_landing_store!(
    /// A day's steps and activity level, a quarter of an hour at a time.
    GarminStepsLandingStore,
    "garmin.steps",
    "garmin_steps_landing"
);
metric_landing_store!(
    /// The walks and other moves the watch detected for itself in a day.
    GarminDetectedMoveLandingStore,
    "garmin.detected_moves",
    "garmin_detected_move_landing"
);

metric_landing_store!(
    /// VO2 max each day it changed, a month to a record.
    GarminVo2MaxLandingStore,
    "garmin.vo2_max",
    "garmin_vo2_max_landing"
);
metric_landing_store!(
    /// The Health Snapshots taken, a month to a record.
    GarminHealthSnapshotLandingStore,
    "garmin.health_snapshots",
    "garmin_health_snapshot_landing"
);
metric_landing_store!(
    /// Fitness age, as it stood each time it changed.
    GarminFitnessAgeLandingStore,
    "garmin.fitness_age",
    "garmin_fitness_age_landing"
);
metric_landing_store!(
    /// Race predictions, as they stood each time they changed.
    GarminRacePredictionLandingStore,
    "garmin.race_predictions",
    "garmin_race_prediction_landing"
);
metric_landing_store!(
    /// Personal records, as they stood each time they changed.
    GarminPersonalRecordLandingStore,
    "garmin.personal_records",
    "garmin_personal_record_landing"
);

#[cfg(test)]
mod tests {
    use super::{
        GarminBodyBatteryEventLandingStore, GarminBodyBatteryLandingStore,
        GarminDailySummaryLandingStore, GarminDetectedMoveLandingStore, GarminFloorsLandingStore,
        GarminIntensityMinutesLandingStore, GarminRespirationLandingStore, GarminSleepLandingStore,
        GarminSpo2LandingStore, GarminStepsLandingStore, GarminStressLandingStore, LandingStream,
    };
    use super::{
        GarminFitnessAgeLandingStore, GarminHealthSnapshotLandingStore,
        GarminPersonalRecordLandingStore, GarminRacePredictionLandingStore,
        GarminVo2MaxLandingStore,
    };

    /// The constant every run's identity is derived from must name a stream.
    #[test]
    fn every_declared_stream_is_a_stream() {
        for stream in [
            GarminSleepLandingStore::STREAM,
            GarminStressLandingStore::STREAM,
            GarminBodyBatteryLandingStore::STREAM,
            GarminBodyBatteryEventLandingStore::STREAM,
            GarminRespirationLandingStore::STREAM,
            GarminSpo2LandingStore::STREAM,
            GarminIntensityMinutesLandingStore::STREAM,
            GarminFloorsLandingStore::STREAM,
            GarminDailySummaryLandingStore::STREAM,
            GarminStepsLandingStore::STREAM,
            GarminDetectedMoveLandingStore::STREAM,
            GarminVo2MaxLandingStore::STREAM,
            GarminHealthSnapshotLandingStore::STREAM,
            GarminFitnessAgeLandingStore::STREAM,
            GarminRacePredictionLandingStore::STREAM,
            GarminPersonalRecordLandingStore::STREAM,
        ] {
            let parsed = LandingStream::try_from(stream).expect("a stream name");
            assert_eq!(parsed.to_string(), stream);
        }
    }
}
