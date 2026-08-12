//! Opening the store, and bringing its schema up to date.

use std::{path::Path, str::FromStr};

use application::StoreError;
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

use super::store_error;

/// Open the store at `path`, creating it if absent, and run every migration.
///
/// The path is a parameter rather than a constant: nothing about where the
/// database lives is compiled in.
///
/// # Errors
///
/// Returns [`StoreError`] if the file cannot be opened or the migrations
/// cannot be applied.
pub async fn connect(path: &Path) -> Result<SqlitePool, StoreError> {
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
        .map_err(|error| store_error(&error))?
        .create_if_missing(true)
        // WAL so that `status` can read while a run is writing. Extraction is
        // single-flight by design, but reading is not, and a status query
        // blocked behind a long fetch would make staleness harder to see
        // rather than easier.
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .map_err(|error| store_error(&error))?;

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .map_err(|error| StoreError::Unavailable {
            detail: error.to_string(),
        })?;

    Ok(pool)
}
