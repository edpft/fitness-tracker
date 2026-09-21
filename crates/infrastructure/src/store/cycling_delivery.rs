//! What cycling sessions were written to a destination, and what went.
//!
//! § 12 authored data, and the only row a cycling prescription leaves. The
//! authored programme says what the session *is*; this says that it was sent,
//! for which slot, and which classes actually went — including the cool-down,
//! which is chosen at the moment of delivery and appears in no programme.

use application::{CyclingDeliveryStore, DestinationName, StoreError};
use domain::{
    cycling::{DeliveredRide, RideVenue, SessionPosition},
    provider::ProgrammeName,
    sequence::NonEmpty,
};
use jiff::civil::Date;
use sqlx::SqlitePool;

use super::store_error;

#[derive(Debug, Clone)]
pub struct SqliteCyclingDeliveryStore {
    pool: SqlitePool,
}

impl SqliteCyclingDeliveryStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

const fn corrupt(detail: String) -> StoreError {
    StoreError::Corrupt { detail }
}

impl CyclingDeliveryStore for SqliteCyclingDeliveryStore {
    async fn delivered_for(
        &self,
        date: Date,
        destination: &DestinationName,
    ) -> Result<Option<DeliveredRide>, StoreError> {
        let prescribed_for = date.to_string();
        let name = destination.to_string();

        let Some(row) = sqlx::query!(
            r#"
            SELECT id           AS "id!: i64",
                   programme    AS "programme: String",
                   microcycle   AS "microcycle!: i64",
                   session      AS "session!: i64",
                   delivered_at AS "delivered_at!: String"
              FROM cycling_delivery
             WHERE prescribed_for = ? AND destination = ?
            "#,
            prescribed_for,
            name,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?
        else {
            return Ok(None);
        };

        let classes = sqlx::query!(
            r#"
            SELECT reference AS "reference!: String",
                   called    AS "called!: String"
              FROM cycling_delivery_class
             WHERE delivery = ?
             ORDER BY position ASC
            "#,
            row.id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let classes: Vec<RideVenue> = classes
            .into_iter()
            .map(|class| {
                RideVenue::new(&class.reference, &class.called)
                    .map_err(|error| corrupt(error.to_string()))
            })
            .collect::<Result<_, _>>()?;

        // A delivery with no class is a delivery that never happened, and the
        // type says so: the write below is one statement, so a row here with
        // no children is a store somebody has edited.
        let classes = NonEmpty::new(classes)
            .map_err(|_| corrupt(format!("a delivery for {prescribed_for} wrote no class")))?;

        let microcycle = u32::try_from(row.microcycle)
            .map_err(|_| corrupt(format!("microcycle {}", row.microcycle)))?;
        let session = u8::try_from(row.session)
            .map_err(|_| corrupt(format!("session {}", row.session)))
            .and_then(|position| {
                SessionPosition::new(position).map_err(|error| corrupt(error.to_string()))
            })?;

        Ok(Some(DeliveredRide {
            prescribed_for: date,
            destination: destination.clone(),
            programme: row
                .programme
                .map(ProgrammeName::try_from)
                .transpose()
                .map_err(|error| corrupt(error.to_string()))?,
            microcycle,
            session,
            classes,
            delivered_at: row
                .delivered_at
                .parse()
                .map_err(|_| corrupt(format!("{:?} does not time a delivery", row.delivered_at)))?,
        }))
    }

    async fn record(&self, delivered: &DeliveredRide) -> Result<(), StoreError> {
        let prescribed_for = delivered.prescribed_for.to_string();
        let destination = delivered.destination.to_string();
        let programme = delivered.programme.as_ref().map(ToString::to_string);
        let microcycle = i64::from(delivered.microcycle);
        let session = i64::from(delivered.session.as_u8());
        let delivered_at = delivered.delivered_at.to_string();

        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        // **The row moves rather than accumulating**, which is what the
        // destination does: Peloton's stack is one list, so delivering again
        // for a date replaces what was sent rather than adding beside it. The
        // classes go with it — they are what *this* delivery wrote.
        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO cycling_delivery
                (prescribed_for, destination, programme, microcycle, session, delivered_at)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT (prescribed_for, destination) DO UPDATE SET
                programme    = excluded.programme,
                microcycle   = excluded.microcycle,
                session      = excluded.session,
                delivered_at = excluded.delivered_at
            RETURNING id AS "id!: i64"
            "#,
            prescribed_for,
            destination,
            programme,
            microcycle,
            session,
            delivered_at,
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| store_error(&error))?;

        sqlx::query!("DELETE FROM cycling_delivery_class WHERE delivery = ?", id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error(&error))?;

        for (position, venue) in delivered.classes.iter().enumerate() {
            let position = i64::try_from(position)
                .map_err(|_| corrupt("more classes than a session can hold".to_owned()))?;
            let reference = venue.reference();
            let called = venue.called();
            sqlx::query!(
                r#"
                INSERT INTO cycling_delivery_class (delivery, position, reference, called)
                VALUES (?, ?, ?, ?)
                "#,
                id,
                position,
                reference,
                called,
            )
            .execute(&mut *transaction)
            .await
            .map_err(|error| store_error(&error))?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| store_error(&error))
    }
}
