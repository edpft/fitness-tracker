//! The normalised layer for `withings.measurements`, and the account raw is
//! derived from.
//!
//! Two adapters and a reader, for the reason [`super::peloton_normalised`] has
//! them: the account reader has no `append`, the store writes the derivation,
//! and the history reads back what relative strength needs.
//!
//! **Which groups make a weigh-in is not decided here.** That is Withings
//! knowledge and lives in [`crate::withings::account`]; this reads rows.

use application::{AccountReader, NormalisedEntityStore, StoreError, WeighInHistory};
use domain::{
    analytical::Weighed,
    body::{BodyScanWeighIn, Composition, MeasuredBy, Rhythm, Segment},
    landing::{
        Endpoint, EventKind, EventProvenance, EventTime, FetchedAt, InvalidStream, LandedRecord,
        LandingRecord, LandingRecordId, LandingStream, Provenance, RawPayload, SourceRecordId,
    },
    measure::Kg,
    normalised::{NormalisationRunId, OperatorZone, StartedAt, WorkoutCount},
};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::withings::{WeighInAccount, group};

use super::{
    WithingsMeasurementLandingStore, corrupt, count_from_storage, normalisation_run_for_storage,
    store_error,
};

/// Raw, read-only, for Withings weigh-ins.
#[derive(Debug, Clone)]
pub struct WithingsWeighInAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl WithingsWeighInAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(WithingsMeasurementLandingStore::STREAM)?,
        })
    }
}

impl AccountReader for WithingsWeighInAccountReader {
    type Account = WeighInAccount;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self) -> Result<Vec<WeighInAccount>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM withings_measurement_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let occurred_at = row
                .event_time
                .as_deref()
                .map(EventTime::try_from)
                .transpose()
                .map_err(|error| corrupt(&error))?;
            let provenance = EventProvenance::new(
                Endpoint::try_from(row.endpoint.as_str()).map_err(|error| corrupt(&error))?,
                EventKind::try_from(row.event_kind.as_str()).map_err(|error| corrupt(&error))?,
                occurred_at,
            );
            let record = LandingRecord::land(
                self.stream.clone(),
                FetchedAt::try_from(row.fetched_at.as_str()).map_err(|error| corrupt(&error))?,
                SourceRecordId::try_from(row.source_record_id.as_str())
                    .map_err(|error| corrupt(&error))?,
                provenance.into(),
                RawPayload::try_from(row.payload).map_err(|error| corrupt(&error))?,
            );
            records.push(LandedRecord::new(
                LandingRecordId::try_from(row.id).map_err(|error| corrupt(&error))?,
                record,
            ));
        }

        Ok(group(records))
    }
}

/// The normalised layer for Body Scan weigh-ins.
#[derive(Debug, Clone)]
pub struct SqliteWeighInStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteWeighInStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(WithingsMeasurementLandingStore::STREAM)?,
        })
    }
}

/// A non-negative quantity on its way into the store.
fn stored(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Corrupt {
        detail: format!("{value} is larger than the store can hold"),
    })
}

fn grams(mass: Kg) -> Result<i64, StoreError> {
    stored(mass.as_grams())
}

impl NormalisedEntityStore for SqliteWeighInStore {
    type Entity = BodyScanWeighIn;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        weigh_ins: Vec<BodyScanWeighIn>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        for statement in [
            "DELETE FROM body_scan_part",
            "DELETE FROM body_scan_vascular",
            "DELETE FROM body_scan_nerves",
            "DELETE FROM body_scan_heart",
            "DELETE FROM body_scan_segment",
            "DELETE FROM body_scan_weigh_in",
        ] {
            sqlx::query(statement)
                .execute(&mut *tx)
                .await
                .map_err(|error| store_error(&error))?;
        }

        let written = weigh_ins.len();
        for weigh_in in weigh_ins {
            write_weigh_in(&mut tx, run_id, &weigh_in).await?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let row = sqlx::query!(r#"SELECT COUNT(*) AS "total!: i64" FROM body_scan_weigh_in"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        count_from_storage(Some(row.total)).map(WorkoutCount::from)
    }
}

/// The weigh-in's own row: when, and what the body is made of.
async fn write_composition(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    measured_at: &StartedAt,
    composition: &Composition,
) -> Result<(), StoreError> {
    let id = composition.from.landed_as.as_i64();
    let zone = measured_at.zone().id().to_owned();
    let measured_at = measured_at.instant().to_string();
    let mass = grams(composition.mass)?;
    let fat_free = grams(composition.fat_free_mass)?;
    let fat = grams(composition.fat_mass)?;
    let muscle = grams(composition.muscle_mass)?;
    let water = grams(composition.body_water)?;
    let extracellular = grams(composition.extracellular_water)?;
    let intracellular = grams(composition.intracellular_water)?;
    let bone = grams(composition.bone_mass)?;
    let visceral = i64::from(composition.visceral_fat.as_tenths());
    let metabolic_rate = i64::from(composition.basal_metabolic_rate.as_u32());
    let metabolic_age = i64::from(composition.metabolic_age.as_tenths_of_a_year());

    sqlx::query!(
        r#"
        INSERT INTO body_scan_weigh_in (
            landing_record_id, measured_at_utc, zone, mass_grams, fat_free_mass_grams,
            fat_mass_grams, muscle_mass_grams, body_water_grams, extracellular_water_grams,
            intracellular_water_grams, bone_mass_grams, visceral_fat_tenths,
            basal_metabolic_rate_kcal, metabolic_age_tenths, run_id
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        id,
        measured_at,
        zone,
        mass,
        fat_free,
        fat,
        muscle,
        water,
        extracellular,
        intracellular,
        bone,
        visceral,
        metabolic_rate,
        metabolic_age,
        run_id,
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(())
}

async fn write_weigh_in(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    weigh_in: &BodyScanWeighIn,
) -> Result<(), StoreError> {
    let composition = weigh_in.composition();
    let id = composition.from.landed_as.as_i64();
    write_composition(tx, run_id, weigh_in.measured_at(), composition).await?;

    let segments = &composition.segments;
    for (name, segment) in [
        ("left-arm", &segments.left_arm),
        ("right-arm", &segments.right_arm),
        ("left-leg", &segments.left_leg),
        ("right-leg", &segments.right_leg),
        ("torso", &segments.torso),
    ] {
        write_segment(tx, id, name, segment).await?;
    }
    write_part(tx, id, "composition", &composition.from).await?;

    if let Some(heart) = weigh_in.heart() {
        let beats = i64::from(heart.heart_rate.as_u32());
        let rhythm = heart.rhythm.map(Rhythm::as_str);
        sqlx::query!(
            r#"
            INSERT INTO body_scan_heart (weigh_in, beats_per_minute, rhythm)
            VALUES (?, ?, ?)
            "#,
            id,
            beats,
            rhythm,
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
        for from in heart.from.iter() {
            write_part(tx, id, "heart", from).await?;
        }
    }

    if let Some(nerves) = weigh_in.nerves() {
        let left = i64::from(nerves.left_foot.as_nanosiemens());
        let right = i64::from(nerves.right_foot.as_nanosiemens());
        let both = i64::from(nerves.both_feet.as_nanosiemens());
        sqlx::query!(
            r#"
            INSERT INTO body_scan_nerves (
                weigh_in, left_foot_nanosiemens, right_foot_nanosiemens, both_feet_nanosiemens
            )
            VALUES (?, ?, ?, ?)
            "#,
            id,
            left,
            right,
            both,
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
        write_part(tx, id, "nerves", &nerves.from).await?;
    }

    if let Some(vascular) = weigh_in.vascular() {
        let velocity = i64::from(vascular.pulse_wave_velocity.as_millimetres_per_second());
        let age = i64::from(vascular.vascular_age.as_tenths_of_a_year());
        sqlx::query!(
            r#"
            INSERT INTO body_scan_vascular (
                weigh_in, pulse_wave_velocity_millimetres_per_second, vascular_age_tenths
            )
            VALUES (?, ?, ?)
            "#,
            id,
            velocity,
            age,
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
        write_part(tx, id, "vascular", &vascular.from).await?;
    }

    Ok(())
}

async fn write_segment(
    tx: &mut Transaction<'_, Sqlite>,
    weigh_in: i64,
    name: &str,
    segment: &Segment,
) -> Result<(), StoreError> {
    let fat_free = grams(segment.fat_free_mass)?;
    let fat = grams(segment.fat_mass)?;
    let muscle = grams(segment.muscle_mass)?;
    sqlx::query!(
        r#"
        INSERT INTO body_scan_segment (
            weigh_in, segment, fat_free_mass_grams, fat_mass_grams, muscle_mass_grams
        )
        VALUES (?, ?, ?, ?, ?)
        "#,
        weigh_in,
        name,
        fat_free,
        fat,
        muscle,
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(())
}

async fn write_part(
    tx: &mut Transaction<'_, Sqlite>,
    weigh_in: i64,
    part: &str,
    from: &MeasuredBy,
) -> Result<(), StoreError> {
    let landed_as = from.landed_as.as_i64();
    let source_record_id = from.source_record_id.as_str().to_owned();
    let algorithm = stored(from.algorithm)?;
    let Provenance::Event(event) = &from.provenance;
    let endpoint = event.endpoint().as_str().to_owned();
    let event_kind = event.kind().as_str().to_owned();
    let event_time = event.occurred_at().map(|at| at.as_timestamp().to_string());
    sqlx::query!(
        r#"
        INSERT INTO body_scan_part (
            landing_record_id, weigh_in, part, source_record_id, algorithm,
            endpoint, event_kind, event_time
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        landed_as,
        weigh_in,
        part,
        source_record_id,
        algorithm,
        endpoint,
        event_kind,
        event_time,
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(())
}

/// Weigh-ins, read back for relative strength.
#[derive(Debug, Clone)]
pub struct SqliteWeighInHistory {
    pool: SqlitePool,
}

impl SqliteWeighInHistory {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl WeighInHistory for SqliteWeighInHistory {
    async fn weigh_ins(&self) -> Result<Vec<Weighed>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT measured_at_utc AS "measured_at!: String",
                   zone AS "zone!: String",
                   mass_grams AS "mass_grams!: i64"
            FROM body_scan_weigh_in
            ORDER BY measured_at_utc ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        rows.into_iter()
            .map(|row| {
                let instant = row
                    .measured_at
                    .parse::<jiff::Timestamp>()
                    .map_err(|error| corrupt(&error))?;
                let zone = OperatorZone::try_from(row.zone).map_err(|error| corrupt(&error))?;
                let grams = u64::try_from(row.mass_grams).map_err(|error| corrupt(&error))?;
                Ok(Weighed {
                    at: StartedAt::new(instant, zone),
                    mass: Kg::from_grams(grams),
                })
            })
            .collect()
    }
}
