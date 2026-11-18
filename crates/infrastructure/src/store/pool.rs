//! Opening the store, and bringing its schema up to date.

use std::{
    collections::HashMap,
    num::NonZeroUsize,
    path::Path,
    str::FromStr,
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use application::StoreError;
use jiff::Timestamp;
use sqlx::{
    SqliteConnection, SqlitePool,
    migrate::{Migrate as _, Migrator},
    pool::PoolConnectionMetadata,
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
/// **Opening a store is also what keeps an unexpected answer** (#164): from
/// here on, an answer to any request this program makes that its reader
/// refuses is written to this store's `unexpected_response`.
///
/// # Errors
///
/// Returns [`StoreError`] if the file cannot be opened, the copy cannot be
/// taken, or the migrations cannot be applied. A copy that fails stops the
/// migration: running it without one is the thing the copy is for.
pub async fn connect(path: &Path) -> Result<SqlitePool, StoreError> {
    Ok(opened(path).await?.0)
}

/// Open the store as [`connect`] does, counting what this process commits to
/// it (#830).
///
/// # Errors
///
/// As [`connect`].
pub async fn connect_counting(path: &Path) -> Result<(SqlitePool, Commits), StoreError> {
    opened(path).await
}

/// The store at `path`, opened and migrated, and what this process commits
/// to it.
async fn opened(path: &Path) -> Result<(SqlitePool, Commits), StoreError> {
    let commits = Commits::default();
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

    let connected = commits.clone();
    let released = commits.clone();
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .after_connect(move |connection, _: PoolConnectionMetadata| {
            let commits = connected.clone();
            Box::pin(async move { commits.watch(connection).await })
        })
        .after_release(move |connection, _: PoolConnectionMetadata| {
            let commits = released.clone();
            Box::pin(async move { commits.released(connection).await })
        })
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

    crate::http::keep_unexpected_responses_in(path);

    Ok((pool, commits))
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

/// How many transactions this process has committed to the store (#830).
///
/// **Two counts, of commits and of commits settled.** SQLite calls its commit
/// hook before the transaction is visible to another connection, so a read
/// made while a commit is between the hook and its connection's release may
/// see the rows before it or after it. A commit is counted at its hook, so
/// anything kept from before it is stale at once, and is settled once its
/// connection is back in the pool: a read is only worth keeping if every
/// commit counted when it began had settled ([`Self::settled`]).
///
/// **Keeping a drawing is not counted** ([`DRAWINGS`]): a drawing is what a
/// page showed, which nothing is derived from, and a page drawn for the first
/// time would otherwise move everything its own drawing read.
///
/// **Only this process's.** Another process writing the store moves nothing
/// here: the operator, 2026-11-18, *"until they do it's okay for the web
/// server to be unaware of changes made by the CLI."*
#[derive(Debug, Clone, Default)]
pub struct Commits(Arc<Counted>);

/// The table a page's drawing is kept in, which nothing is derived from.
const DRAWINGS: &str = "kept_drawing";

#[derive(Debug, Default)]
struct Counted {
    /// Transactions committed, each counted at its commit hook.
    committed: AtomicU64,
    /// Of those, the ones whose connection has been released since.
    settled: AtomicU64,
    /// What each open connection has written, by the address of its handle.
    connections: Mutex<HashMap<NonZeroUsize, Arc<Mutex<Written>>>>,
}

/// What one connection has written: in the transaction it has open, and
/// committed since it was last released.
#[derive(Debug, Default)]
struct Written {
    /// A row in a drawing was written in the open transaction.
    drawings: bool,
    /// A row anywhere else was written in the open transaction.
    elsewhere: bool,
    /// Transactions that count, committed since the last release.
    committed: u64,
}

impl Written {
    /// The open transaction committed: whether it counts, which it does
    /// unless all it was seen to write was drawings.
    ///
    /// **SQLite does not report a write to a table without a rowid**, so one
    /// seen to write nothing counts, and a drawing is kept in a transaction of
    /// its own: one writing a drawing and such a table would read as drawings
    /// alone.
    const fn commit(&mut self) -> bool {
        let counts = self.elsewhere || !self.drawings;
        if counts {
            self.committed += 1;
        }
        self.drawings = false;
        self.elsewhere = false;
        counts
    }

    const fn roll_back(&mut self) {
        self.drawings = false;
        self.elsewhere = false;
    }
}

impl Commits {
    /// How many transactions this process has committed.
    pub fn committed(&self) -> u64 {
        self.0.committed.load(Ordering::Acquire)
    }

    /// How many transactions this process has committed, where every one of
    /// them has settled, and `None` while one is still to.
    pub fn settled(&self) -> Option<u64> {
        let committed = self.committed();
        (self.0.settled.load(Ordering::Acquire) == committed).then_some(committed)
    }

    /// Note what `connection` writes and commits.
    async fn watch(&self, connection: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        let written = Arc::new(Mutex::new(Written::default()));
        let mut handle = connection.lock_handle().await?;
        self.0
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(handle.as_raw_handle().addr(), Arc::clone(&written));
        let updated = Arc::clone(&written);
        handle.set_update_hook(move |update| {
            let mut written = updated.lock().unwrap_or_else(PoisonError::into_inner);
            if update.table == DRAWINGS {
                written.drawings = true;
            } else {
                written.elsewhere = true;
            }
        });
        let rolled_back = Arc::clone(&written);
        handle.set_rollback_hook(move || {
            rolled_back
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .roll_back();
        });
        let counted = Arc::clone(&self.0);
        handle.set_commit_hook(move || {
            if written
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .commit()
            {
                counted.committed.fetch_add(1, Ordering::AcqRel);
            }
            true
        });
        Ok(())
    }

    /// Settle what `connection` committed before it was released, and keep it.
    async fn released(&self, connection: &mut SqliteConnection) -> Result<bool, sqlx::Error> {
        let address = connection.lock_handle().await?.as_raw_handle().addr();
        let written = self
            .0
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&address)
            .cloned();
        let committed = written.map_or(0, |written| {
            std::mem::take(
                &mut written
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .committed,
            )
        });
        self.0.settled.fetch_add(committed, Ordering::AcqRel);
        Ok(true)
    }
}
