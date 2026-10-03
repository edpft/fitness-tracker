//! What was delivered, and where.
//!
//! § 12 authored data rather than a derivation: the reference is minted by a
//! system we do not own, so losing it costs a fact rather than a recomputation
//! and nothing here rebuilds it.
//!
//! Everything interesting about a delivery is in the destination that performed
//! it; what is here is which prescription holds which of a destination's places,
//! the one write that moves a place from one prescription to another, and what
//! the destination said each time it was asked.

use application::{
    DeliveryReference, DestinationName, DestinationReply, Occupant, PrescribedWorkoutId,
    PrescriptionDeliveryStore, PrescriptionLifecycle, RenderingDigest, StoreError,
};
use domain::prescription::PrescriptionState;
use jiff::{Timestamp, civil::Date};
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

    async fn occupying(
        &self,
        date: Date,
        destination: &DestinationName,
    ) -> Result<Option<Occupant>, StoreError> {
        let issued_for = date.to_string();
        let destination = destination.to_string();

        // The join is the whole of it: a delivery belongs to a prescription and
        // a prescription is issued for a date, so a date's occupant is whatever
        // delivery hangs off any of that date's issues. There is at most one,
        // because a replacement hands the place over rather than adding a row —
        // and `LIMIT 1` with the newest first is a belt for a state the writes
        // do not create rather than a choice between candidates.
        //
        // **Four facts, one query.** Whether a workout names the place decides
        // whether what is in it may be replaced, and asking that separately
        // would let it change between the two reads — the same reason
        // `state_of` below answers both of its questions at once.
        let row = sqlx::query!(
            r#"
            SELECT d.prescription AS "prescription!: i64",
                   d.reference    AS "reference!: String",
                   d.rendering    AS "rendering: Vec<u8>",
                   EXISTS (
                       SELECT 1 FROM gym_workout AS w
                       WHERE w.performed_against = d.reference
                   ) AS "performed!: i64"
            FROM prescription_delivery AS d
            JOIN prescribed_workout AS p ON p.id = d.prescription
            WHERE p.issued_for = ? AND d.destination = ?
            ORDER BY d.delivered_at DESC
            LIMIT 1
            "#,
            issued_for,
            destination
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let reference =
            DeliveryReference::try_from(row.reference).map_err(|error| StoreError::Corrupt {
                detail: error.to_string(),
            })?;

        // A row written before this build kept the digest holds null, which is
        // a real state: the routine exists and what it says is not on record.
        // Anything else of the wrong width is a file this program did not write.
        let rendering = row
            .rendering
            .as_deref()
            .map(RenderingDigest::try_from)
            .transpose()
            .map_err(|error| StoreError::Corrupt {
                detail: error.to_string(),
            })?;

        Ok(Some(Occupant {
            prescription: PrescribedWorkoutId::new(row.prescription),
            reference,
            rendering,
            performed: row.performed != 0,
        }))
    }

    async fn hand_over(
        &self,
        from: PrescribedWorkoutId,
        to: PrescribedWorkoutId,
        destination: &DestinationName,
        reference: &DeliveryReference,
        rendering: &RenderingDigest,
        at: Timestamp,
    ) -> Result<(), StoreError> {
        let from = from.as_i64();
        let to = to.as_i64();
        let destination = destination.to_string();
        let reference = reference.to_string();
        let rendering = rendering.as_bytes().as_slice();
        let delivered_at = at.to_string();

        // **Delete then insert, in one transaction, rather than an update.**
        // The delete is what the performed-session trigger watches, so routing
        // the hand-over through it is what makes "a performed session is not
        // replaced" hold here without a check in the use case — an `UPDATE ...
        // SET prescription = ?` would slide straight past it.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        sqlx::query!(
            "DELETE FROM prescription_delivery WHERE prescription = ? AND destination = ?",
            from,
            destination
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;

        sqlx::query!(
            r#"
            INSERT INTO prescription_delivery
                (prescription, destination, reference, rendering, delivered_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
            to,
            destination,
            reference,
            rendering,
            delivered_at
        )
        .execute(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?;

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(())
    }

    async fn record(
        &self,
        prescription: PrescribedWorkoutId,
        destination: &DestinationName,
        reference: &DeliveryReference,
        rendering: &RenderingDigest,
        at: Timestamp,
    ) -> Result<(), StoreError> {
        let id = prescription.as_i64();
        let destination = destination.to_string();
        let reference = reference.to_string();
        let rendering = rendering.as_bytes().as_slice();
        let delivered_at = at.to_string();

        // A plain insert, with no upsert clause. A second delivery of one
        // prescription to one destination is a defect rather than a state to
        // reconcile, and the primary key saying so out loud is better than a
        // silent overwrite that leaves an orphaned routine behind it. The one
        // legitimate restatement goes through `record_rendering`, which keeps
        // the place and changes what is in it.
        sqlx::query!(
            r#"
            INSERT INTO prescription_delivery
                (prescription, destination, reference, rendering, delivered_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
            id,
            destination,
            reference,
            rendering,
            delivered_at
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        Ok(())
    }

    async fn record_rendering(
        &self,
        prescription: PrescribedWorkoutId,
        destination: &DestinationName,
        reference: &DeliveryReference,
        rendering: &RenderingDigest,
        at: Timestamp,
    ) -> Result<(), StoreError> {
        let id = prescription.as_i64();
        let destination = destination.to_string();
        let reference = reference.to_string();
        let rendering = rendering.as_bytes().as_slice();
        let delivered_at = at.to_string();

        // **An update, and the one write here that is.** The hand-over above
        // goes through a delete so the trigger pinning a performed session
        // sees it; nothing is withdrawn here, because the place keeps its
        // occupant and the receipt a performance joins through is the
        // reference — which this restates rather than retires. What stops a
        // performed session being rewritten is the use case declining to send
        // one, which is where § 12 can be stated as a reason rather than as an
        // abort.
        sqlx::query!(
            r#"
            UPDATE prescription_delivery
            SET reference = ?, rendering = ?, delivered_at = ?
            WHERE prescription = ? AND destination = ?
            "#,
            reference,
            rendering,
            delivered_at,
            id,
            destination
        )
        .execute(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        Ok(())
    }

    async fn record_reply(
        &self,
        prescription: PrescribedWorkoutId,
        destination: &DestinationName,
        reply: &DestinationReply,
        at: Timestamp,
    ) -> Result<(), StoreError> {
        let id = prescription.as_i64();
        let destination = destination.to_string();
        let status = reply.status().as_str();
        let succeeded = i64::from(reply.status().succeeded());
        let body = reply.body();
        let answered_at = at.to_string();

        // An insert with nothing to reconcile against, because there is nothing
        // it could collide with: this table has no key but its own, on purpose.
        // Every attempt leaves a row, and two attempts that said the same thing
        // are two answers rather than one repeated.
        sqlx::query!(
            r#"
            INSERT INTO delivery_reply
                (prescription, destination, status, succeeded, body, answered_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
            id,
            destination,
            status,
            succeeded,
            body,
            answered_at
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
