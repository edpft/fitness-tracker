//! Each chart page as it was last drawn, by the address it is served at
//! (#605).
//!
//! **Reconstructible, so governed by nothing** (§ II): losing a drawing costs a
//! page drawn again, never a fact. A page drawn again replaces the drawing
//! before it, and nothing keeps the one replaced.
//!
//! **No port, because no use case needs one**, as with the operator's settings:
//! a drawing is what a frontend made of what the use cases answered, and
//! nothing in the core reads it back.

use application::StoreError;
use sqlx::SqlitePool;

use super::store_error;

/// The kept drawings, in the store.
#[derive(Debug, Clone)]
pub struct SqliteKeptDrawings {
    pool: SqlitePool,
}

impl SqliteKeptDrawings {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Keep `drawing` as the page at `address`, in place of any kept before.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn keep(&self, address: &str, drawing: &str) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            INSERT INTO kept_drawing (address, drawing)
            VALUES (?1, ?2)
            ON CONFLICT (address) DO UPDATE SET drawing = excluded.drawing
            "#,
            address,
            drawing,
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }

    /// The page at `address` as it was last kept, if it has been.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn kept(&self, address: &str) -> Result<Option<String>, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT drawing AS "drawing!: String"
            FROM kept_drawing
            WHERE address = ?1
            "#,
            address,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(row.map(|row| row.drawing))
    }
}
