//! A store about to be migrated is copied first (#151).
//!
//! Asserted from the outside: what is beside the store file afterwards, and what
//! the copy holds. A store one migration behind is made by running every
//! migration but the last, which is the state an upgraded binary finds.

use std::path::{Path, PathBuf};

use infrastructure::connect;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

/// The copies beside the store, by the name `connect` gives them.
fn copies_beside(store: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let Some(directory) = store.parent() else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().contains(".before-"))
        {
            found.push(path);
        }
    }
    Ok(found)
}

/// A store with every migration applied except the last.
async fn one_migration_behind(path: &Path) -> Result<i64, Box<dyn std::error::Error>> {
    let migrator = sqlx::migrate!("../../migrations");
    let versions: Vec<i64> = migrator.iter().map(|migration| migration.version).collect();
    let [.., previous, _] = versions.as_slice() else {
        return Err("fewer than two migrations".into());
    };
    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true),
    )
    .await?;
    migrator.run_to(*previous, &pool).await?;
    pool.close().await;
    Ok(*previous)
}

async fn latest_applied(path: &Path) -> Result<i64, Box<dyn std::error::Error>> {
    let pool = SqlitePool::connect_with(SqliteConnectOptions::new().filename(path)).await?;
    let latest: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await?;
    pool.close().await;
    Ok(latest)
}

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

#[test]
fn a_store_behind_the_binary_is_copied_as_it_stood() {
    runtime().expect("a runtime is available").block_on(async {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let store = directory.path().join("store.db");
        let previous = one_migration_behind(&store)
            .await
            .expect("a store one migration behind");

        let pool = connect(&store).await.expect("the store opens and migrates");
        pool.close().await;

        let copies = copies_beside(&store).expect("the directory is readable");
        let [copy] = copies.as_slice() else {
            panic!("exactly one copy beside the store, found {copies:?}");
        };
        assert_eq!(
            latest_applied(copy).await.expect("the copy opens"),
            previous,
            "the copy is the store before the migration, not after it",
        );
        assert!(
            latest_applied(&store).await.expect("the store opens") > previous,
            "the store itself was still migrated",
        );
    });
}

#[test]
fn a_new_store_is_not_copied() {
    runtime().expect("a runtime is available").block_on(async {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let store = directory.path().join("store.db");

        let pool = connect(&store).await.expect("a new store opens");
        pool.close().await;

        assert_eq!(
            copies_beside(&store).expect("the directory is readable"),
            Vec::<PathBuf>::new(),
            "a store nothing had been applied to holds nothing to lose",
        );
    });
}

#[test]
fn a_store_with_nothing_pending_is_not_copied() {
    runtime().expect("a runtime is available").block_on(async {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let store = directory.path().join("store.db");

        connect(&store)
            .await
            .expect("a new store opens")
            .close()
            .await;
        connect(&store).await.expect("it opens again").close().await;

        assert_eq!(
            copies_beside(&store).expect("the directory is readable"),
            Vec::<PathBuf>::new(),
            "reopening an up-to-date store copies nothing",
        );
    });
}
