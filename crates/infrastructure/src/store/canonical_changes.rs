//! The days whose normalised accounts changed since a canonical layer was
//! last derived, by layer (#610).
//!
//! **Written by the normalised writers, in their own transactions**, and read
//! and forgotten by the canonical derivation. Beside the layers rather than in
//! them, as the derivation ledger is: losing it would leave a layer standing
//! with days it has not rebuilt, so only the derivations touch it.

use std::collections::BTreeSet;

use application::{CanonicalLayer, StoreError};
use jiff::civil::Date;
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{corrupt, store_error};

/// Record that the accounts on these days changed.
pub(super) async fn record(
    tx: &mut Transaction<'_, Sqlite>,
    layer: CanonicalLayer,
    days: &BTreeSet<Date>,
) -> Result<(), StoreError> {
    let layer = layer.as_str();
    for day in days {
        let day = day.to_string();
        sqlx::query!(
            "INSERT INTO canonical_changed_day (layer, day) VALUES (?, ?) ON CONFLICT DO NOTHING",
            layer,
            day
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
    }
    Ok(())
}

/// Every day recorded for the layer.
pub(super) async fn days(
    pool: &SqlitePool,
    layer: CanonicalLayer,
) -> Result<Vec<Date>, StoreError> {
    let layer = layer.as_str();
    let days = sqlx::query_scalar!(
        r#"SELECT day AS "day!: String" FROM canonical_changed_day WHERE layer = ? ORDER BY day"#,
        layer
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;
    days.iter()
        .map(|day| day.parse::<Date>().map_err(|e| corrupt(&e)))
        .collect()
}

/// Forget these days, once the derivation has rebuilt what they reach. Only
/// these: a writer may have recorded more since they were read.
pub(super) async fn forget(
    tx: &mut Transaction<'_, Sqlite>,
    layer: CanonicalLayer,
    days: &BTreeSet<Date>,
) -> Result<(), StoreError> {
    let layer = layer.as_str();
    for day in days {
        let day = day.to_string();
        sqlx::query!(
            "DELETE FROM canonical_changed_day WHERE layer = ? AND day = ?",
            layer,
            day
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
    }
    Ok(())
}

/// Forget every day recorded for the layer, which a whole derivation follows.
pub(super) async fn forget_all(
    tx: &mut Transaction<'_, Sqlite>,
    layer: CanonicalLayer,
) -> Result<(), StoreError> {
    let layer = layer.as_str();
    sqlx::query!("DELETE FROM canonical_changed_day WHERE layer = ?", layer)
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
    Ok(())
}
