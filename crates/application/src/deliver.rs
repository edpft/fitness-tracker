//! Putting an issued prescription where the operator trains from.
//!
//! Generic over its ports, so the use case knows nothing about routines,
//! folders or HTTP. What it knows is that a session was issued for a date, that
//! a destination will take it and give it a name, and that the name is worth
//! keeping.
//!
//! **Delivery derives nothing.** It reads what `prescribe` already issued and
//! does not issue anything itself — so a destination being unreachable costs a
//! retry rather than a ladder position, and running it twice cannot advance
//! anything. § 36 is the reason: the prescription is in the store before this
//! use case contacts anything, and stays there whatever the destination does.
//!
//! **One delivery per issued prescription**, which falls out of § 12 rather than
//! out of anything a destination imposes: an issued prescription is written once
//! and never rewritten, a reissue is a *different* prescription, and so a
//! session that should be replaced is a new delivery while a session asked about
//! twice is the same one. That the destination in use also cannot delete what it
//! has been given is a happy agreement, not the reason.
//!
//! **What makes "a session asked about twice" true is decision 0021**, and it is
//! worth naming because the guard below is keyed on the prescription's identity
//! rather than on what it says. `prescribe` derives on every run; a derivation
//! that produces the same `WorkoutShape` is not issued, so it does not get an
//! identity, so it cannot reach this as a second delivery. Were that not so,
//! every run of the daily loop would put another routine on the operator's
//! phone.

use domain::prescription::{DeliveryReference, SessionOrdinal};
use jiff::{Timestamp, civil::Date};

use crate::{
    error::DeliveryError,
    ports::{
        Deliverable, Delivery, PrescribedWorkoutStore, PrescriptionDeliverer,
        PrescriptionDeliveryStore, PrescriptionDestination, ProgrammeStore,
    },
};

/// Everything delivery needs from the outside.
pub struct DeliveryPorts<S, P, D, T> {
    pub prescriptions: S,
    pub programmes: P,
    pub deliveries: D,
    pub destination: T,
}

/// The use case.
pub struct Delivering<S, P, D, T> {
    ports: DeliveryPorts<S, P, D, T>,
}

impl<S, P, D, T> Delivering<S, P, D, T> {
    pub const fn new(ports: DeliveryPorts<S, P, D, T>) -> Self {
        Self { ports }
    }
}

impl<S, P, D, T> PrescriptionDeliverer for Delivering<S, P, D, T>
where
    S: PrescribedWorkoutStore + Sync,
    P: ProgrammeStore + Sync,
    D: PrescriptionDeliveryStore + Sync,
    T: PrescriptionDestination + Sync,
{
    async fn deliver(&self, date: Date) -> Result<Delivery, DeliveryError> {
        let destination = self.ports.destination.name();

        let (id, workout) = self
            .ports
            .prescriptions
            .issued_for(date)
            .await?
            .ok_or(DeliveryError::NothingIssued { date })?;

        // The programme supplies what the prescription does not carry: its name,
        // and which session of it this is. Both are facts about the calendar
        // rather than about what was issued, which is why they are derived here
        // and not stored on the prescription.
        let (_, programme) = self
            .ports
            .programmes
            .on(date)
            .await?
            .ok_or(DeliveryError::NoProgramme { date })?;

        let ordinal = programme
            .calendar()
            .ordinal(date)
            .ok_or(DeliveryError::NoProgramme { date })?;

        // **Asked before sent.** Without this, a second invocation leaves the
        // operator two sessions for one date and no way to tell which is in
        // force — and the destination in use cannot delete either of them.
        if let Some(reference) = self.ports.deliveries.reference_for(id, destination).await? {
            return Ok(already_delivered(reference, destination.clone(), ordinal));
        }

        let delivered = self
            .ports
            .destination
            .deliver(&Deliverable {
                workout,
                programme: programme.name().clone(),
                ordinal,
            })
            .await?;

        self.ports
            .deliveries
            .record(id, destination, &delivered.reference, Timestamp::now())
            .await?;

        Ok(Delivery {
            reference: delivered.reference,
            destination: destination.clone(),
            ordinal,
            freshly_delivered: true,
            unexpressed: delivered.unexpressed,
        })
    }
}

/// What was already there.
///
/// **`unexpressed` is empty rather than remembered.** What a destination could
/// not state is a fact about a rendering that happened, and this call performed
/// none — reporting the previous run's omissions as though they were this one's
/// would be inventing an observation. The operator saw them when the session was
/// delivered.
const fn already_delivered(
    reference: DeliveryReference,
    destination: crate::ports::DestinationName,
    ordinal: SessionOrdinal,
) -> Delivery {
    Delivery {
        reference,
        destination,
        ordinal,
        freshly_delivered: false,
        unexpressed: Vec::new(),
    }
}
