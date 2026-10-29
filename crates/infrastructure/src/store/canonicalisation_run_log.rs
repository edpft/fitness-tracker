//! What happened on each rebuild of a canonical layer (#609).

use application::{
    CanonicalLayer, CanonicalisationOutcome, CanonicalisationRunId, CanonicalisationRunLog,
    StoreError,
};
use domain::landing::FetchedAt;
use sqlx::SqlitePool;

use super::{count_for_storage, store_error};

/// The rebuild history.
#[derive(Debug, Clone)]
pub struct SqliteCanonicalisationRunLog {
    pool: SqlitePool,
}

impl SqliteCanonicalisationRunLog {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl CanonicalisationRunLog for SqliteCanonicalisationRunLog {
    async fn begin(
        &self,
        layer: CanonicalLayer,
        at: FetchedAt,
    ) -> Result<CanonicalisationRunId, StoreError> {
        let layer = layer.as_str();
        let started_at = at.to_string();
        let id = sqlx::query!(
            "INSERT INTO canonicalisation_run (layer, started_at) VALUES (?, ?) RETURNING id",
            layer,
            started_at
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?
        .id;
        Ok(CanonicalisationRunId::from(id))
    }

    async fn finish(
        &self,
        run: CanonicalisationRunId,
        outcome: CanonicalisationOutcome,
    ) -> Result<(), StoreError> {
        let id = run.as_i64();
        match outcome {
            CanonicalisationOutcome::Succeeded {
                finished_at,
                read,
                written,
            } => {
                let finished_at = finished_at.to_string();
                let read = count_for_storage(read)?;
                let written = count_for_storage(written)?;
                sqlx::query!(
                    r#"
                    UPDATE canonicalisation_run
                    SET finished_at = ?, outcome = 'succeeded', read = ?, written = ?
                    WHERE id = ?
                    "#,
                    finished_at,
                    read,
                    written,
                    id
                )
                .execute(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;
            }
            CanonicalisationOutcome::Failed { finished_at } => {
                let finished_at = finished_at.to_string();
                sqlx::query!(
                    "UPDATE canonicalisation_run SET finished_at = ?, outcome = 'failed' WHERE id = ?",
                    finished_at,
                    id
                )
                .execute(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;
            }
        }
        Ok(())
    }
}
