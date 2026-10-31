//! What each derived layer standing in the store was derived from (#468).
//!
//! **Beside the layers, never part of them.** Losing any of it costs one
//! derivation of every layer and nothing else, which is why a normalised
//! layer's inputs sit on the run that wrote it: a derivation started by hand
//! records none, and the layer it wrote reads as derived from nothing known.

use application::{CanonicalLayer, DerivationLedger, DerivedFrom, StoreError};
use domain::{landing::LandingStream, normalised::NormalisationRunId};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{normalisation_run_for_storage, store_error};

/// The ledger, over the run log and a table of its own.
#[derive(Debug, Clone)]
pub struct SqliteDerivationLedger {
    pool: SqlitePool,
}

impl SqliteDerivationLedger {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl DerivationLedger for SqliteDerivationLedger {
    /// **The latest success, ordered as the run log orders it**, because that
    /// is the derivation whose rows the layer holds: a failed one writes
    /// nothing.
    async fn normalised(&self, stream: &LandingStream) -> Result<Option<DerivedFrom>, StoreError> {
        let stream = stream.to_string();
        let row = sqlx::query!(
            r#"
            SELECT derived_from AS "derived_from: String"
            FROM normalisation_run
            WHERE stream = ? AND outcome = 'succeeded'
            ORDER BY finished_at DESC, id DESC
            LIMIT 1
            "#,
            stream
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(row.and_then(|row| row.derived_from).map(DerivedFrom::from))
    }

    async fn record_normalised(
        &self,
        run: NormalisationRunId,
        from: &DerivedFrom,
    ) -> Result<(), StoreError> {
        let run = normalisation_run_for_storage(run)?;
        let from = from.as_str();
        sqlx::query!(
            "UPDATE normalisation_run SET derived_from = ? WHERE id = ?",
            from,
            run
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }

    async fn canonical(&self, layer: CanonicalLayer) -> Result<Option<DerivedFrom>, StoreError> {
        let layer = layer.as_str();
        let from = sqlx::query_scalar!(
            "SELECT derived_from FROM canonical_derivation WHERE layer = ?",
            layer
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(from.map(DerivedFrom::from))
    }

    async fn record_canonical(
        &self,
        layer: CanonicalLayer,
        from: &DerivedFrom,
    ) -> Result<(), StoreError> {
        let layer = layer.as_str();
        let from = from.as_str();
        sqlx::query!(
            "INSERT INTO canonical_derivation (layer, derived_from) VALUES (?, ?)
             ON CONFLICT (layer) DO UPDATE SET derived_from = excluded.derived_from",
            layer,
            from
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }
}

/// Forget what a canonical layer was derived from, in the transaction that
/// empties it (#606).
///
/// **Whatever empties a layer forgets its derivation**, so the next run
/// derives it again whatever its inputs read. A normalised writer clears the
/// whole canonical layer before it removes rows the layer names, even where it
/// then removes none of them: a spreadsheet revised for its weigh-ins alone
/// empties the gym layer and leaves every gym session's run and count as they
/// were. Judged by its inputs alone, the empty layer would read as current.
pub(super) async fn forget(
    tx: &mut Transaction<'_, Sqlite>,
    layer: CanonicalLayer,
) -> Result<(), StoreError> {
    let layer = layer.as_str();
    sqlx::query!("DELETE FROM canonical_derivation WHERE layer = ?", layer)
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    Ok(())
}

/// The derivations that wrote a normalised layer's rows, as a canonical layer
/// built from them is derived from: each stream and its run, in order, and how
/// many rows each one's still stand.
///
/// **The count as well as the run** (#571). A derivation that revises only
/// what changed can remove rows without writing any — a retraction, or a
/// session now refused — and it clears the canonical layer when it does. The
/// runs alone would then read the same as before, and the empty layer as
/// current.
pub(crate) fn written_by(runs: impl IntoIterator<Item = (String, i64, i64)>) -> DerivedFrom {
    let runs: Vec<String> = runs
        .into_iter()
        .map(|(stream, run, rows)| format!("{stream} {run} ({rows})"))
        .collect();
    DerivedFrom::from(runs.join(", "))
}
