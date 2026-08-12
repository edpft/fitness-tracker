//! Where each stream resumes from.
//!
//! Reconstructible state, and stored precisely so it can be thrown away:
//! `clear` is a supported operation rather than a repair, because losing this
//! costs a re-fetch and never a fact.

use application::{ResumptionPointStore, StoreError};
use domain::landing::{FetchedAt, LandingStream, Watermark};
use sqlx::SqlitePool;

use super::store_error;

#[derive(Debug, Clone)]
pub struct SqliteResumptionPointStore {
    pool: SqlitePool,
}

impl SqliteResumptionPointStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl ResumptionPointStore for SqliteResumptionPointStore {
    async fn read(&self, stream: &LandingStream) -> Result<Option<Watermark>, StoreError> {
        let stream = stream.to_string();
        let row = sqlx::query!(
            r#"SELECT watermark AS "watermark!: String" FROM resumption_point WHERE stream = ?"#,
            stream
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        row.map(|row| {
            Watermark::parse(&row.watermark).map_err(|error| StoreError::Corrupt {
                detail: error.to_string(),
            })
        })
        .transpose()
    }

    async fn advance(
        &self,
        stream: &LandingStream,
        to: Watermark,
        at: FetchedAt,
    ) -> Result<(), StoreError> {
        let stream = stream.to_string();
        let watermark = to.to_string();
        let updated_at = at.to_string();

        sqlx::query!(
            r#"
            INSERT INTO resumption_point (stream, watermark, updated_at)
            VALUES (?, ?, ?)
            ON CONFLICT (stream) DO UPDATE
                SET watermark = excluded.watermark,
                    updated_at = excluded.updated_at
            "#,
            stream,
            watermark,
            updated_at
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        Ok(())
    }

    async fn clear(&self, stream: &LandingStream) -> Result<(), StoreError> {
        let stream = stream.to_string();
        // Deleting the row is the whole of a reset. Nothing in raw is touched:
        // the next run re-serves every payload, and identical payloads land
        // nothing.
        sqlx::query!(r#"DELETE FROM resumption_point WHERE stream = ?"#, stream)
            .execute(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;

        Ok(())
    }
}
