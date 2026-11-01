//! What 0044 does to the rows 0043 left behind (#187, #188), and what 0102
//! does to an illness recorded as an alteration (#674).
//!
//! The operator's store had 0043 applied before either defect was found, so its
//! alterations are all holidays with a reason and no way to say the ordinary
//! week stands. This asserts the copy across from the outside: a store one
//! migration behind is written to in 0043's shape, migrated, and read back
//! through the store the binary actually uses.

use std::{collections::BTreeMap, path::Path};

use application::DiaryStore as _;
use domain::schedule::Illness;
use infrastructure::{SqliteDiaryStore, connect};
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};

/// A store migrated as far as 0043, which is the state 0044 has to cope with.
///
/// **Pinned to 0043 rather than to "the last but one"**, which is what it said
/// until 0045 landed and made "the last" a migration this test knows nothing
/// about. What this suite is about is one join — the rows 0043 wrote, read back
/// after 0044 — and that join does not move when a later migration arrives.
async fn before_0044(path: &Path) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    const BEFORE: i64 = 43;

    let migrator = sqlx::migrate!("../../migrations");
    let versions: Vec<i64> = migrator.iter().map(|migration| migration.version).collect();
    for wanted in [BEFORE, 44] {
        if !versions.contains(&wanted) {
            return Err(format!("migration {wanted:04} is missing").into());
        }
    }
    let pool = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true),
    )
    .await?;
    migrator.run_to(BEFORE, &pool).await?;
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
/// stands" — which nothing could say before 0044. And since 0102 the illness
/// is its own record, ended the day after the one day it covered.
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

        let [holiday] = diary.alterations() else {
            panic!(
                "only the holiday is an alteration, found {:?}",
                diary.alterations()
            );
        };

        let ill = Illness::new(jiff::civil::Date::constant(2026, 9, 19))
            .ended(jiff::civil::Date::constant(2026, 9, 20));
        assert_eq!(
            diary.illnesses(),
            [ill.expect("it ends after it begins")],
            "an illness is still an illness, and is no longer asked why",
        );

        assert_eq!(
            holiday.reason(),
            "Rome",
            "a holiday keeps the reason it was given",
        );
        assert!(
            holiday.slots().is_some(),
            "and states its slots, because every row 0043 kept did",
        );
    });
}

/// **The operator's own illness, as 0101 left it** (#674): an alteration from
/// 17 September for five days, which 0102 makes an illness from the 17th that
/// he was better from on the 22nd. His two holidays are untouched.
#[test]
fn an_illness_alteration_becomes_an_ended_illness() {
    runtime().expect("a runtime is available").block_on(async {
        let directory = tempfile::tempdir().expect("a scratch directory");
        let path = directory.path().join("store.db");

        let migrator = sqlx::migrate!("../../migrations");
        let old = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(true),
        )
        .await
        .expect("a scratch store");
        migrator.run_to(101, &old).await.expect("migrated to 0101");
        sqlx::query(
            r"
            INSERT INTO alteration (authored_at, start_date, days, absence, zone, states_slots, reason)
            VALUES ('2026-08-25T10:49:45Z', '2026-08-26', 7, 'holiday', NULL, 1, 'No holiday, no gym access'),
                   ('2026-08-25T10:50:43Z', '2026-09-11', 4, 'holiday', 'Europe/Rome', 1, 'No holiday, no gym access'),
                   ('2026-09-22T08:58:57Z', '2026-09-17', 5, 'illness', NULL, 1, NULL)
            ",
        )
        .execute(&old)
        .await
        .expect("0101 accepts his three alterations");
        old.close().await;

        let pool = connect(&path).await.expect("the store opens and migrates");
        let diary = SqliteDiaryStore::new(pool)
            .diary()
            .await
            .expect("the diary reads back");

        let ill = Illness::new(jiff::civil::Date::constant(2026, 9, 17)).ended(jiff::civil::Date::constant(2026, 9, 22));
        assert_eq!(diary.illnesses(), [ill.expect("it ends after it begins")]);
        assert_eq!(
            diary
                .alterations()
                .iter()
                .map(|alteration| (alteration.start().to_string(), alteration.days().get()))
                .collect::<Vec<_>>(),
            [("2026-08-26".to_owned(), 7), ("2026-09-11".to_owned(), 4)],
            "the holidays are untouched",
        );
        assert!(
            diary
                .alterations()
                .iter()
                .all(|alteration| alteration.slots().is_some_and(BTreeMap::is_empty)),
            "and still leave no room to train",
        );
    });
}
