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
    gym::ManualGymSession,
    landing::{InvalidStream, LandingStream},
    normalised::{NormalisationRunId, WorkoutCount},
};
use sqlx::SqlitePool;

use crate::spreadsheets::SpreadsheetEntity;

use super::{
    SpreadsheetFileLandingStore, count_from_storage,
    manual_session::write_session,
    normalisation_run_for_storage,
    normalised::clear_stream,
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
                SpreadsheetEntity::GymSession(session) => sessions.push(*session),
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
