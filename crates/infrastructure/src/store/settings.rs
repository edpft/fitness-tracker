//! What the operator states once rather than on every invocation.
//!
//! **The store, because there is nowhere better and one place is the point.**
//! A file would have to be found, parsed and kept in step with a schema, for a
//! single value nothing else can supply.
//!
//! **Superseded by date, never overwritten**, as the generation parameters are
//! (§ 12). Moving house changes the zone trained in from a date, and a
//! prescription issued before that was issued against the old one — so the
//! superseded row is the only thing that can explain it.
//!
//! **No port, because no use case needs one.** A zone is read while working out
//! what an invocation means, which is composition rather than application; the
//! use cases are handed an `OperatorZone` already resolved. Declaring a port for
//! it in `application` would be a port no use case calls.
//!
//! **The store's own location is not in here**, and cannot be: it is what has to
//! be known before any of this can be read. It stays a flag, a variable, and the
//! specification's default.

use application::StoreError;
use jiff::Timestamp;
use sqlx::SqlitePool;

use super::store_error;

/// The settings, in the store.
#[derive(Debug, Clone)]
pub struct SqliteOperatorSettingsStore {
    pool: SqlitePool,
}

impl SqliteOperatorSettingsStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The zone in force, if the operator has ever stated one.
    ///
    /// Unparsed, deliberately: a zone that will not resolve is reported the same
    /// way whether it came from a flag, a variable or here, and that means one
    /// place doing the parsing rather than three.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn timezone(&self) -> Result<Option<String>, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT timezone AS "timezone!: String"
            FROM operator_settings
            ORDER BY authored_at DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        Ok(row.map(|row| row.timezone))
    }

    /// State the zone, from now.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn author(&self, at: Timestamp, timezone: &str) -> Result<(), StoreError> {
        let authored_at = at.to_string();
        let timezone = timezone.to_owned();
        sqlx::query!(
            r#"
            INSERT INTO operator_settings (authored_at, timezone)
            VALUES (?1, ?2)
            ON CONFLICT (authored_at) DO UPDATE SET timezone = excluded.timezone
            "#,
            authored_at,
            timezone
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        Ok(())
    }
}
