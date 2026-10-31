//! The canonical layer for weigh-ins (#409), and the normalised layer as it
//! reads it.
//!
//! **Everything downstream reads body weight here**: relative strength reads
//! [`WeighInHistory`] from these tables, so a day the spreadsheets hold in six
//! copies is one weigh-in to it rather than six.
//!
//! **Replacement rather than update**, as every derivation is, and re-deriving
//! any source's weigh-ins clears this layer first: every row names `weigh_in`
//! rows that the re-derivation deletes and rewrites under new ids. See
//! [`clear`].

use std::collections::BTreeSet;

use application::{
    CanonicalLayer, CanonicalWeighInStore, NormalisedWeighInReader, StoreError, WeighInHistory,
};
use domain::{
    analytical::Weighed,
    body::{self, CanonicalWeighIn, NormalisedWeighInId, Stated, WeighInAccount, WeighInCount},
    canonical::{Occurred, Reach},
    measure::Kg,
    normalised::{OperatorZone, StartedAt},
};
use jiff::{Timestamp, civil::Date};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{corrupt, count_from_storage, store_error};

/// Every normalised weigh-in, as the canonical layer reads it.
#[derive(Debug, Clone)]
pub struct SqliteNormalisedWeighInReader {
    pool: SqlitePool,
}

impl SqliteNormalisedWeighInReader {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl NormalisedWeighInReader for SqliteNormalisedWeighInReader {
    async fn on(&self, days: &BTreeSet<Date>) -> Result<Vec<WeighInAccount>, StoreError> {
        // **The whole read, filtered**: a weigh-in is one row with nothing
        // beneath it, so reading every one costs less than working out which.
        let mut accounts = self.all().await?;
        accounts.retain(|account| days.contains(&account.occurred.day()));
        Ok(accounts)
    }

    async fn written_by(&self) -> Result<application::DerivedFrom, StoreError> {
        let runs = sqlx::query!(
            r#"
            SELECT stream AS "stream!: String", run_id AS "run!: i64", count(*) AS "rows!: i64"
            FROM weigh_in
            GROUP BY stream, run_id
            ORDER BY stream, run_id
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(super::derivation_ledger::written_by(
            runs.into_iter().map(|row| (row.stream, row.run, row.rows)),
        ))
    }

    async fn all(&self) -> Result<Vec<WeighInAccount>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT w.id AS "id!: i64",
                   w.on_day AS "on_day: String",
                   w.measured_at_utc AS "measured_at_utc: String",
                   w.zone AS "zone: String",
                   w.mass_grams AS "mass_grams!: i64",
                   f.entered_at_utc AS "entered_at_utc?: String",
                   m.weigh_in AS "copied?: i64",
                   m.last_saved_at_utc AS "last_saved_at_utc?: String"
            FROM weigh_in w
            LEFT JOIN myfitnesspal_weigh_in f ON f.weigh_in = w.id
            LEFT JOIN manual_weigh_in m ON m.weigh_in = w.id
            ORDER BY w.id
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        rows.into_iter()
            .map(|row| {
                let occurred = occurred(
                    row.measured_at_utc.as_deref(),
                    row.zone.as_deref(),
                    row.on_day.as_deref(),
                )?;
                let stated = match (&occurred, row.entered_at_utc, row.copied) {
                    (Occurred::At(_), None, None) => Stated::Measured,
                    (Occurred::On(_), Some(entered), None) => Stated::Entered(instant(&entered)?),
                    (Occurred::On(_), None, Some(_)) => {
                        Stated::Copied(row.last_saved_at_utc.as_deref().map(instant).transpose()?)
                    }
                    _ => {
                        return Err(StoreError::Corrupt {
                            detail: format!(
                                "weigh-in {} is neither measured, entered nor copied",
                                row.id
                            ),
                        });
                    }
                };
                Ok(WeighInAccount {
                    id: weigh_in_id(row.id)?,
                    occurred,
                    mass: mass(row.mass_grams)?,
                    stated,
                })
            })
            .collect()
    }
}

/// The canonical weigh-in layer, in SQLite.
#[derive(Debug, Clone)]
pub struct SqliteCanonicalWeighInStore {
    pool: SqlitePool,
}

impl SqliteCanonicalWeighInStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl CanonicalWeighInStore for SqliteCanonicalWeighInStore {
    async fn replace(&self, weigh_ins: Vec<CanonicalWeighIn>) -> Result<WeighInCount, StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        clear(&mut tx).await?;
        write_all(&mut tx, &weigh_ins).await?;
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WeighInCount::from(weigh_ins.len()))
    }

    async fn reach(&self) -> Result<Reach, StoreError> {
        let days = super::canonical_changes::days(&self.pool, CanonicalLayer::WeighIns).await?;
        Ok(Reach::of(days, body::REACHES))
    }

    async fn replace_within(
        &self,
        reach: &Reach,
        weigh_ins: Vec<CanonicalWeighIn>,
    ) -> Result<WeighInCount, StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        remove_on(&mut tx, reach.rebuilt()).await?;
        write_all(&mut tx, &weigh_ins).await?;
        super::canonical_changes::forget(&mut tx, CanonicalLayer::WeighIns, reach.changed())
            .await?;
        tx.commit().await.map_err(|error| store_error(&error))?;
        self.count().await
    }

    async fn count(&self) -> Result<WeighInCount, StoreError> {
        let count =
            sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!: i64" FROM canonical_weigh_in"#)
                .fetch_one(&self.pool)
                .await
                .map_err(|error| store_error(&error))?;
        Ok(WeighInCount::from(count_from_storage(Some(count))?))
    }
}

/// Each weighing and the accounts it stands on.
async fn write_all(
    tx: &mut Transaction<'_, Sqlite>,
    weigh_ins: &[CanonicalWeighIn],
) -> Result<(), StoreError> {
    for weigh_in in weigh_ins {
        let (measured_at_utc, zone, on_day) = match weigh_in.occurred().value() {
            Occurred::At(at) => (
                Some(at.instant().to_string()),
                Some(at.zone().id().to_owned()),
                None,
            ),
            Occurred::On(day) => (None, None, Some(day.to_string())),
        };
        let occurred_weigh_in = weigh_in.occurred().taken_from().as_i64();
        let mass_grams =
            i64::try_from(weigh_in.mass().value().as_grams()).map_err(|error| corrupt(&error))?;
        let mass_weigh_in = weigh_in.mass().taken_from().as_i64();

        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO canonical_weigh_in (
                on_day, measured_at_utc, zone, occurred_weigh_in, mass_grams, mass_weigh_in
            )
            VALUES (?, ?, ?, ?, ?, ?)
            RETURNING id AS "id!: i64"
            "#,
            on_day,
            measured_at_utc,
            zone,
            occurred_weigh_in,
            mass_grams,
            mass_weigh_in,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;

        let accounts: Vec<i64> = weigh_in.accounts().iter().map(|id| id.as_i64()).collect();
        for account in accounts {
            sqlx::query!(
                "INSERT INTO canonical_weigh_in_account (canonical, weigh_in) VALUES (?, ?)",
                id,
                account,
            )
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        }
    }
    Ok(())
}

/// Body weight, read back for relative strength: one weigh-in per weighing.
///
/// **And its fat-free mass** (#253), as body composition works it out: what an
/// account stated, or else the mass that account weighed less its fat, stated
/// as a mass or a percentage (the operator, 2026-10-27). A stated figure comes
/// first, then the account the mass came from.
impl WeighInHistory for SqliteCanonicalWeighInStore {
    async fn weigh_ins(&self) -> Result<Vec<Weighed>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT c.measured_at_utc AS "measured_at_utc: String",
                   c.zone AS "zone: String",
                   c.on_day AS "on_day: String",
                   c.mass_grams AS "mass_grams!: i64",
                   (SELECT CASE
                             WHEN stated.mass_grams IS NOT NULL THEN stated.mass_grams
                             WHEN fat.mass_grams IS NOT NULL
                                 THEN w.mass_grams - fat.mass_grams
                             ELSE w.mass_grams * (1000 - fat.percentage_tenths) / 1000
                           END
                    FROM canonical_weigh_in_account a
                    JOIN weigh_in w ON w.id = a.weigh_in
                    LEFT JOIN body_composition stated
                      ON stated.weigh_in = a.weigh_in AND stated.component = 'fat-free'
                    LEFT JOIN body_composition fat
                      ON fat.weigh_in = a.weigh_in AND fat.component = 'fat'
                    WHERE a.canonical = c.id
                      AND (stated.mass_grams IS NOT NULL
                           OR (fat.mass_grams IS NOT NULL AND fat.mass_grams < w.mass_grams)
                           OR (fat.percentage_tenths IS NOT NULL
                               AND fat.percentage_tenths < 1000))
                    ORDER BY stated.mass_grams IS NULL,
                             a.weigh_in <> (SELECT m.mass_weigh_in FROM canonical_weigh_in m
                                            WHERE m.id = a.canonical),
                             a.weigh_in
                    LIMIT 1) AS "fat_free_grams: i64"
            FROM canonical_weigh_in c
            ORDER BY COALESCE(c.measured_at_utc, c.on_day), c.id
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        rows.into_iter()
            .map(|row| {
                Ok(Weighed {
                    at: occurred(
                        row.measured_at_utc.as_deref(),
                        row.zone.as_deref(),
                        row.on_day.as_deref(),
                    )?,
                    mass: mass(row.mass_grams)?,
                    fat_free: row.fat_free_grams.map(mass).transpose()?,
                })
            })
            .collect()
    }
}

/// Everything the layer holds, in foreign-key order.
///
/// **Called by every writer of `weigh_in`**, inside its own transaction,
/// because each row here names rows that writer is about to delete.
pub(super) async fn clear(tx: &mut Transaction<'_, Sqlite>) -> Result<(), StoreError> {
    for statement in [
        "DELETE FROM canonical_weigh_in_account",
        "DELETE FROM canonical_weigh_in",
    ] {
        sqlx::query(statement)
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
    }
    // A whole derivation follows, so no day is left to rebuild alone.
    super::canonical_changes::forget_all(tx, CanonicalLayer::WeighIns).await?;
    super::derivation_ledger::forget(tx, CanonicalLayer::WeighIns).await
}

/// Record that the normalised weigh-ins on these days changed, and empty the
/// days that reaches (#610), in the transaction that changes them: a weighing
/// there may name a weigh-in the caller is about to delete.
pub(super) async fn changed(
    tx: &mut Transaction<'_, Sqlite>,
    days: BTreeSet<Date>,
) -> Result<(), StoreError> {
    if days.is_empty() {
        return Ok(());
    }
    super::canonical_changes::record(tx, CanonicalLayer::WeighIns, &days).await?;
    remove_on(tx, Reach::of(days, body::REACHES).rebuilt()).await
}

/// Record the days of the weigh-ins these rows name: each row's day, or its
/// instant's where its zone puts it.
pub(super) async fn changed_rows(
    tx: &mut Transaction<'_, Sqlite>,
    rows: impl IntoIterator<Item = (Option<String>, Option<String>, Option<String>)>,
) -> Result<(), StoreError> {
    let days = rows
        .into_iter()
        .map(|(measured_at_utc, zone, on_day)| {
            occurred(
                measured_at_utc.as_deref(),
                zone.as_deref(),
                on_day.as_deref(),
            )
            .map(|when| when.day())
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    changed(tx, days).await
}

/// The days of these normalised weigh-ins, given as a JSON array of their ids,
/// recorded as changed (#610). Called before they are deleted.
pub(super) async fn changed_weigh_ins(
    tx: &mut Transaction<'_, Sqlite>,
    weigh_ins: &str,
) -> Result<(), StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT measured_at_utc AS "measured_at_utc: String", zone AS "zone: String",
               on_day AS "on_day: String"
          FROM weigh_in WHERE id IN (SELECT value FROM json_each(?))
        "#,
        weigh_ins
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    changed_rows(
        tx,
        rows.into_iter()
            .map(|row| (row.measured_at_utc, row.zone, row.on_day)),
    )
    .await
}

/// The days of every weigh-in one stream's run just wrote, recorded as changed
/// (#610).
pub(super) async fn changed_by_run(
    tx: &mut Transaction<'_, Sqlite>,
    stream: &str,
    run_id: i64,
) -> Result<(), StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT measured_at_utc AS "measured_at_utc: String", zone AS "zone: String",
               on_day AS "on_day: String"
          FROM weigh_in WHERE stream = ? AND run_id = ?
        "#,
        stream,
        run_id
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    changed_rows(
        tx,
        rows.into_iter()
            .map(|row| (row.measured_at_utc, row.zone, row.on_day)),
    )
    .await
}

/// Every weighing on one of these days, the accounts it names first.
async fn remove_on(
    tx: &mut Transaction<'_, Sqlite>,
    days: &BTreeSet<Date>,
) -> Result<(), StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT id AS "id!: i64", measured_at_utc AS "measured_at_utc: String",
               zone AS "zone: String", on_day AS "on_day: String"
          FROM canonical_weigh_in
        "#
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    let mut weighings = Vec::new();
    for row in rows {
        let when = occurred(
            row.measured_at_utc.as_deref(),
            row.zone.as_deref(),
            row.on_day.as_deref(),
        )?;
        if days.contains(&when.day()) {
            weighings.push(row.id);
        }
    }
    if weighings.is_empty() {
        return Ok(());
    }
    let weighings = super::ids_json(weighings);
    for statement in [
        "DELETE FROM canonical_weigh_in_account WHERE canonical IN (SELECT value FROM json_each(?))",
        "DELETE FROM canonical_weigh_in WHERE id IN (SELECT value FROM json_each(?))",
    ] {
        sqlx::query(statement)
            .bind(&weighings)
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
    }
    Ok(())
}

/// When a weigh-in happened: at an instant in a zone, or on a day.
fn occurred(
    measured_at: Option<&str>,
    zone: Option<&str>,
    on_day: Option<&str>,
) -> Result<Occurred, StoreError> {
    match (measured_at, zone, on_day) {
        (Some(measured_at), Some(zone), None) => {
            let zone = OperatorZone::try_from(zone.to_owned()).map_err(|error| corrupt(&error))?;
            Ok(Occurred::At(StartedAt::new(instant(measured_at)?, zone)))
        }
        (None, None, Some(on_day)) => Ok(Occurred::On(
            on_day.parse::<Date>().map_err(|error| corrupt(&error))?,
        )),
        _ => Err(StoreError::Corrupt {
            detail: "a weigh-in is neither at an instant nor on a day".to_owned(),
        }),
    }
}

fn instant(text: &str) -> Result<Timestamp, StoreError> {
    text.parse::<Timestamp>().map_err(|error| corrupt(&error))
}

fn mass(grams: i64) -> Result<Kg, StoreError> {
    Ok(Kg::from_grams(
        u64::try_from(grams).map_err(|error| corrupt(&error))?,
    ))
}

fn weigh_in_id(id: i64) -> Result<NormalisedWeighInId, StoreError> {
    NormalisedWeighInId::try_from(id).map_err(|error| corrupt(&error))
}
