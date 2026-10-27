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

use application::{CanonicalWeighInStore, NormalisedWeighInReader, StoreError, WeighInHistory};
use domain::{
    analytical::Weighed,
    body::{CanonicalWeighIn, NormalisedWeighInId, Stated, WeighInAccount, WeighInCount},
    canonical::Occurred,
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
        for weigh_in in &weigh_ins {
            let (measured_at_utc, zone, on_day) = match weigh_in.occurred().value() {
                Occurred::At(at) => (
                    Some(at.instant().to_string()),
                    Some(at.zone().id().to_owned()),
                    None,
                ),
                Occurred::On(day) => (None, None, Some(day.to_string())),
            };
            let occurred_weigh_in = weigh_in.occurred().taken_from().as_i64();
            let mass_grams = i64::try_from(weigh_in.mass().value().as_grams())
                .map_err(|error| corrupt(&error))?;
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
            .fetch_one(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

            let accounts: Vec<i64> = weigh_in.accounts().iter().map(|id| id.as_i64()).collect();
            for account in accounts {
                sqlx::query!(
                    "INSERT INTO canonical_weigh_in_account (canonical, weigh_in) VALUES (?, ?)",
                    id,
                    account,
                )
                .execute(&mut *tx)
                .await
                .map_err(|error| store_error(&error))?;
            }
        }
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WeighInCount::from(weigh_ins.len()))
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
