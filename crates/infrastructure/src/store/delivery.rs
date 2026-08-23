//! What was delivered, and where.
//!
//! § 12 authored data rather than a derivation: the reference is minted by a
//! system we do not own, so losing it costs a fact rather than a recomputation
//! and nothing here rebuilds it.
//!
//! The whole adapter is two statements, which is the point — everything
//! interesting about a delivery is in the destination that performed it.

use application::{
    DeliveryReference, DestinationName, PrescribedWorkoutId, PrescriptionDeliveryStore, StoreError,
};
use jiff::Timestamp;
use sqlx::SqlitePool;

use super::store_error;

#[derive(Debug, Clone)]
pub struct SqlitePrescriptionDeliveryStore {
    pool: SqlitePool,
}

impl SqlitePrescriptionDeliveryStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl PrescriptionDeliveryStore for SqlitePrescriptionDeliveryStore {
    async fn reference_for(
        &self,
        prescription: PrescribedWorkoutId,
        destination: &DestinationName,
    ) -> Result<Option<DeliveryReference>, StoreError> {
        let id = prescription.as_i64();
        let destination = destination.to_string();

        let row = sqlx::query!(
            r#"
            SELECT reference AS "reference!: String"
            FROM prescription_delivery
            WHERE prescription = ? AND destination = ?
            "#,
            id,
            destination
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        row.map(|row| {
            DeliveryReference::try_from(row.reference).map_err(|error| StoreError::Corrupt {
                detail: error.to_string(),
            })
        })
        .transpose()
    }

    async fn record(
        &self,
        prescription: PrescribedWorkoutId,
        destination: &DestinationName,
        reference: &DeliveryReference,
        at: Timestamp,
    ) -> Result<(), StoreError> {
        let id = prescription.as_i64();
        let destination = destination.to_string();
        let reference = reference.to_string();
        let delivered_at = at.to_string();

        // A plain insert, with no upsert clause. A second delivery of one
        // prescription to one destination is a defect rather than a state to
        // reconcile, and the primary key saying so out loud is better than a
        // silent overwrite that leaves an orphaned routine behind it.
        sqlx::query!(
            r#"
            INSERT INTO prescription_delivery
                (prescription, destination, reference, delivered_at)
            VALUES (?, ?, ?, ?)
            "#,
            id,
            destination,
            reference,
            delivered_at
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        Ok(())
    }
}
