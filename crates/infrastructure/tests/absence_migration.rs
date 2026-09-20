//! What 0044 does to the rows 0043 left behind (#187, #188).
//!
//! The operator's store had 0043 applied before either defect was found, so its
//! alterations are all holidays with a reason and no way to say the ordinary
//! week stands. This asserts the copy across from the outside: a store one
//! migration behind is written to in 0043's shape, migrated, and read back
//! through the store the binary actually uses.

use std::path::Path;

use application::DiaryStore as _;
use domain::schedule::Absence;
use infrastructure::{SqliteDiaryStore, connect};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

/// A store with every migration applied except the last, which is 0044.
async fn before_0044(path: &Path) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    let migrator = sqlx::migrate!("../../migrations");
    let versions: Vec<i64> = migrator.iter().map(|migration| migration.version).collect();
    let [.., previous, last] = versions.as_slice() else {
        return Err("fewer than two migrations".into());
    };
    if *last != 44 {
        return Err(format!("0044 is no longer the last migration, {last} is").into());
    }
    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true),
    )
    .await?;
    migrator.run_to(*previous, &pool).await?;
    Ok(pool)
}

/// One alteration in 0043's shape, which has no `states_slots` and demands a
/// reason of every kind.
async fn insert_0043(
    pool: &SqlitePool,
    start: &str,
    absence: &str,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r"
        INSERT INTO alteration (authored_at, start_date, days, absence, zone, reason)
        VALUES ('2026-09-19T21:00:00Z', ?, 1, ?, NULL, ?)
        ",
    )
    .bind(start)
    .bind(absence)
    .bind(reason)
    .execute(pool)
    .await
    .map(|_| ())
}

fn runtime() -> Result<tokio::runtime::Runtime, std::io::Error> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

/// **The operator's illness loses the reason he was asked for, and nothing else
/// is lost.** #187: a reason belongs to a holiday. #188: every row 0043 kept
/// states its slots, so migrating cannot turn one into "the ordinary week
/// stands" — which nothing could say before 0044.
#[test]
fn an_illness_reads_back_without_its_reason() {
    runtime().expect("a runtime is available").block_on(async {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let path = directory.path().join("store.db");

        let old = before_0044(&path).await.expect("a store before 0044");
        insert_0043(&old, "2026-09-19", "illness", "I was ill")
            .await
            .expect("0043 accepts an illness with a reason");
        insert_0043(&old, "2026-09-14", "holiday", "Rome")
            .await
            .expect("0043 accepts a holiday");
        old.close().await;

        let pool = connect(&path).await.expect("the store opens and migrates");
        let diary = SqliteDiaryStore::new(pool)
            .diary()
            .await
            .expect("the diary reads back");

        let [holiday, illness] = diary.alterations() else {
            panic!("both alterations survive, found {:?}", diary.alterations());
        };

        assert_eq!(
            illness.absence(),
            &Absence::Illness,
            "an illness is still an illness",
        );
        assert_eq!(illness.reason(), None, "and is no longer asked why");

        assert_eq!(
            holiday.reason(),
            Some("Rome"),
            "a holiday keeps the reason it was given",
        );
        assert!(
            holiday.slots().is_some(),
            "and states its slots, because every row 0043 kept did",
        );
    });
}
