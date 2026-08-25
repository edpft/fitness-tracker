//! What was delivered, and where.
//!
//! § 12 authored data rather than a derivation: the reference is minted by a
//! system we do not own, so losing it costs a fact rather than a recomputation
//! and nothing here rebuilds it.
//!
//! The whole adapter is two statements, which is the point — everything
//! interesting about a delivery is in the destination that performed it.

use application::{
    DeliveryReference, DestinationName, PrescribedWorkoutId, PrescriptionDeliveryStore,
    PrescriptionLifecycle, StoreError,
};
use domain::prescription::PrescriptionState;
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

impl PrescriptionLifecycle for SqlitePrescriptionDeliveryStore {
    async fn state_of(
        &self,
        prescription: PrescribedWorkoutId,
    ) -> Result<PrescriptionState, StoreError> {
        let id = prescription.as_i64();

        // One query for both questions: has it been delivered, and does any
        // workout name what it was delivered as. Two would let the answer
        // change between them.
        let row = sqlx::query!(
            r#"
            SELECT d.reference AS "reference!: String",
                   EXISTS (
                       SELECT 1 FROM gym_workout AS w
                       WHERE w.performed_against = d.reference
                   ) AS "performed!: i64"
            FROM prescription_delivery AS d
            WHERE d.prescription = ?
            ORDER BY d.delivered_at DESC
            LIMIT 1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        // No delivery: nothing outside this store knows it exists.
        let Some(row) = row else {
            return Ok(PrescriptionState::Drafted);
        };

        let reference =
            DeliveryReference::try_from(row.reference).map_err(|error| StoreError::Corrupt {
                detail: error.to_string(),
            })?;

        Ok(if row.performed == 0 {
            PrescriptionState::Published { reference }
        } else {
            PrescriptionState::Performed { reference }
        })
    }
}
