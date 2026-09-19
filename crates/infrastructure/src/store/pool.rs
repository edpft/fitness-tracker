//! Opening the store, and bringing its schema up to date.

use std::{path::Path, str::FromStr};

use application::StoreError;
use jiff::Timestamp;
use sqlx::{
    SqlitePool,
    migrate::{Migrate as _, Migrator},
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};

use super::store_error;

/// sqlx's own name for the table recording what has been applied. Its default,
/// and nothing here changes it.
const APPLIED: &str = "_sqlx_migrations";

/// Open the store at `path`, creating it if absent, and run every migration.
///
/// The path is a parameter rather than a constant: nothing about where the
/// database lives is compiled in.
///
/// **A store about to be migrated is copied first** (#151). Some of what it
/// holds is authored and nothing upstream can re-fetch it, and a migration can
/// fail, or succeed and carry less than it should. The copy is taken by the
/// program rather than remembered by the operator, so every migration gets one.
///
/// # Errors
///
/// Returns [`StoreError`] if the file cannot be opened, the copy cannot be
/// taken, or the migrations cannot be applied. A copy that fails stops the
/// migration: running it without one is the thing the copy is for.
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

    let migrator = sqlx::migrate!("../../migrations");
    back_up_before_migrating(&pool, &migrator, path).await?;

    migrator
        .run(&pool)
        .await
        .map_err(|error| StoreError::Unavailable {
            detail: error.to_string(),
        })?;

    Ok(pool)
}

/// Copy the store beside itself if a migration is about to run on it.
///
/// **Nothing applied is nothing to lose.** A store no migration has touched is
/// one being created, and copying it would leave an empty file beside every new
/// store. A store with nothing pending is not about to change.
///
/// Named for the first migration it precedes and the moment it was taken, so a
/// second attempt at a migration that failed does not collide with the first
/// copy, and the copy that matters is the oldest with that number.
async fn back_up_before_migrating(
    pool: &SqlitePool,
    migrator: &Migrator,
    path: &Path,
) -> Result<(), StoreError> {
    let unavailable = |error: &dyn std::fmt::Display| StoreError::Unavailable {
        detail: error.to_string(),
    };

    let mut connection = pool.acquire().await.map_err(|error| store_error(&error))?;
    connection
        .ensure_migrations_table(APPLIED)
        .await
        .map_err(|error| unavailable(&error))?;
    let applied = connection
        .list_applied_migrations(APPLIED)
        .await
        .map_err(|error| unavailable(&error))?;
    if applied.is_empty() {
        return Ok(());
    }

    let Some(first_pending) = migrator
        .iter()
        .map(|migration| migration.version)
        .find(|version| !applied.iter().any(|done| done.version == *version))
    else {
        return Ok(());
    };

    let taken = Timestamp::now().strftime("%Y%m%dT%H%M%SZ");
    let name = path.file_name().map_or_else(
        || "store".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    let copy = path.with_file_name(format!("{name}.before-{first_pending:04}-{taken}"));
    let destination = copy.to_string_lossy().into_owned();

    // Unchecked rather than `query!`: a `VACUUM` returns no columns, so there is
    // nothing for the macro to check, and it reads a consistent snapshot —
    // write-ahead log included — which copying the file would not.
    sqlx::query("VACUUM INTO ?1")
        .bind(destination)
        .execute(&mut *connection)
        .await
        .map_err(|error| store_error(&error))?;

    Ok(())
}
