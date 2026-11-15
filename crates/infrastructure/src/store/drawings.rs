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

use application::{DrawnFrom, StoreError};
use sqlx::SqlitePool;

use super::store_error;

/// A page as it was last drawn, and what it was drawn from, where that was
/// known (#805).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptDrawing {
    pub drawing: String,
    pub drawn_from: Option<DrawnFrom>,
}

/// The kept drawings, in the store.
#[derive(Debug, Clone)]
pub struct SqliteKeptDrawings {
    pool: SqlitePool,
}

impl SqliteKeptDrawings {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Keep `drawing` as the page at `address`, drawn from `drawn_from`, in
    /// place of any kept before.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn keep(
        &self,
        address: &str,
        drawn_from: Option<DrawnFrom>,
        drawing: &str,
    ) -> Result<(), StoreError> {
        let drawn_from = drawn_from.as_ref().map(AsRef::<[u8]>::as_ref);
        sqlx::query!(
            r#"
            INSERT INTO kept_drawing (address, drawing, drawn_from)
            VALUES (?1, ?2, ?3)
            ON CONFLICT (address) DO UPDATE
               SET drawing = excluded.drawing,
                   drawn_from = excluded.drawn_from
            "#,
            address,
            drawing,
            drawn_from,
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
    pub async fn kept(&self, address: &str) -> Result<Option<KeptDrawing>, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT drawing AS "drawing!: String", drawn_from
            FROM kept_drawing
            WHERE address = ?1
            "#,
            address,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        row.map(|row| {
            let drawn_from = row
                .drawn_from
                .as_deref()
                .map(DrawnFrom::try_from)
                .transpose()
                .map_err(|_| StoreError::Corrupt {
                    detail: format!("the drawing kept at {address} has no digest"),
                })?;
            Ok(KeptDrawing {
                drawing: row.drawing,
                drawn_from,
            })
        })
        .transpose()
    }
}
