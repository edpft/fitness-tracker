//! The normalised layer for `spreadsheets.files`: manual weigh-ins and manual
//! gym sessions, written together.
//!
//! **One derivation, two kinds of entity.** One landed file holds both, and
//! the stream has one run log, one set of refusals and one "records behind". Two
//! derivations of the one stream would each replace the other's refusals and
//! share a run log, so the files are read once and everything they hold is
//! written here in one transaction (#274).
//!
//! **Each kind goes in its own source-independent table** (§ 3.1): a weigh-in
//! beside the Body Scan's in `weigh_in`, a session beside Hevy's in the gym
//! tables. The `stream` column tells them apart, and only this stream's rows are
//! replaced.

use application::{NormalisedEntityStore, StoreError};
use domain::{
    body::ManualWeighIn,
    gym::{ManualExercise, ManualGymSession},
    landing::{InvalidStream, LandingStream},
    normalised::{NormalisationRunId, WorkoutCount},
};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::spreadsheets::SpreadsheetEntity;

use super::{
    SpreadsheetFileLandingStore, count_for_storage, count_from_storage,
    normalisation_run_for_storage,
    normalised::{Row, clear_stream, write_set},
    store_error,
    weigh_in::{count_manual_weigh_ins, replace_manual_weigh_ins},
};

/// Writes what the spreadsheets derive.
#[derive(Debug, Clone)]
pub struct SqliteSpreadsheetStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteSpreadsheetStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(SpreadsheetFileLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteSpreadsheetStore {
    type Entity = SpreadsheetEntity;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        entities: Vec<SpreadsheetEntity>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let stream = self.stream.to_string();
        let written = entities.len();

        let mut weigh_ins: Vec<ManualWeighIn> = Vec::new();
        let mut sessions: Vec<ManualGymSession> = Vec::new();
        for entity in entities {
            match entity {
                SpreadsheetEntity::WeighIn(weigh_in) => weigh_ins.push(weigh_in),
                SpreadsheetEntity::GymSession(session) => sessions.push(session),
            }
        }

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        replace_manual_weigh_ins(&mut tx, run_id, &stream, &weigh_ins).await?;
        clear_stream(&mut tx, &stream).await?;
        for session in &sessions {
            write_session(&mut tx, run_id, &stream, session).await?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    /// Weigh-ins and sessions together, because that is what a run derives.
    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let stream = self.stream.to_string();
        let row = sqlx::query!(
            r#"SELECT count(*) AS "count!: i64" FROM gym_session WHERE stream = ?"#,
            stream
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let sessions = count_from_storage(Some(row.count))?;
        let weigh_ins = count_manual_weigh_ins(&self.pool).await?;
        Ok(WorkoutCount::from(weigh_ins.saturating_add(sessions)))
    }
}

/// One session: the session row, the one workout row that carries its day,
/// and an item per exercise.
///
/// **One workout per session.** A workout is a part of a session as one source
/// recorded it, and a workbook records a session in one place, so there is only
/// ever one part. It names the most recent copy of the workbook that holds the
/// session; each set names the copy it was taken from.
async fn write_session(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    stream: &str,
    session: &ManualGymSession,
) -> Result<(), StoreError> {
    let logged = session.logged();
    let landed_as = logged.landed_as.as_i64();
    let source_record_id = logged.source_record_id.as_str();
    let on_day = session.on().to_string();

    let row = sqlx::query!(
        r#"
        INSERT INTO gym_session (stream, landing_record_id, run_id) VALUES (?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        stream,
        landed_as,
        run_id
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    let session_id = row.id;

    let row = sqlx::query!(
        r#"
        INSERT INTO gym_workout (
            session, stream, landing_record_id, source_record_id, on_day, run_id
        )
        VALUES (?, ?, ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        session_id,
        stream,
        landed_as,
        source_record_id,
        on_day,
        run_id
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    let workout = row.id;

    for (position, exercise) in session.exercises().iter().enumerate() {
        let position = count_for_storage(position)?;
        write_exercise(tx, workout, position, exercise).await?;
    }
    Ok(())
}

async fn write_exercise(
    tx: &mut Transaction<'_, Sqlite>,
    workout: i64,
    position: i64,
    exercise: &ManualExercise,
) -> Result<(), StoreError> {
    let key = exercise.exercise_key();
    let measure = exercise.measure();

    sqlx::query!(
        "INSERT INTO workout_item (workout, position, is_superset) VALUES (?, ?, 0)",
        workout,
        position
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    sqlx::query!(
        r#"
        INSERT INTO performed_exercise (workout, item_position, position, exercise, measure)
        VALUES (?, ?, 0, ?, ?)
        "#,
        workout,
        position,
        key,
        measure
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    match exercise {
        ManualExercise::ForReps { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let ordinal = count_for_storage(ordinal)?;
                let reps = set.outcome.completed().map(|reps| i64::from(reps.as_u32()));
                write_set(tx, Row::manual(workout, position, ordinal, set))
                    .reps(reps)
                    .execute()
                    .await?;
            }
        }
        ManualExercise::ForDuration { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let ordinal = count_for_storage(ordinal)?;
                let seconds = set
                    .outcome
                    .completed()
                    .map(|duration| i64::try_from(duration.as_seconds()))
                    .transpose()
                    .map_err(|_| StoreError::Corrupt {
                        detail: "a duration larger than the store can hold".to_owned(),
                    })?;
                write_set(tx, Row::manual(workout, position, ordinal, set))
                    .duration(seconds)
                    .execute()
                    .await?;
            }
        }
        ManualExercise::ForDistance { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let ordinal = count_for_storage(ordinal)?;
                let millimetres = set
                    .outcome
                    .completed()
                    .map(|metres| i64::try_from(metres.as_millimetres()))
                    .transpose()
                    .map_err(|_| StoreError::Corrupt {
                        detail: "a distance larger than the store can hold".to_owned(),
                    })?;
                write_set(tx, Row::manual(workout, position, ordinal, set))
                    .distance(millimetres)
                    .execute()
                    .await?;
            }
        }
    }
    Ok(())
}
