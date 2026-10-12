//! The gym's prescription use cases, over this build's adapters.
//!
//! **Prescription is not a stream**, so none of these takes a credential except
//! delivery, and that one is handed its destination: which sink a discipline
//! delivers to is the catalogue's to say.

use application::{
    Delivery, DeliveryError, MesocycleStore, PlanStore, PrescribedWorkoutStore,
    PrescriptionDeliverer as _, PrescriptionDeliveryStore, PrescriptionDestination,
    compare::{Comparing, ComparisonPorts},
    deliver::{Delivering, DeliveryPorts},
    prescribe::{Prescribing, PrescriptionPorts},
    reschedule::Rescheduled,
};
use domain::normalised::OperatorZone;
use infrastructure::HevyRoutinePreview;
use jiff::civil::Date;

use crate::{next::NextFailure, rescheduling, store::Store};

/// The gym's mesocycles, as the plan now stands.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub async fn programmes(
    store: &Store,
    zone: &OperatorZone,
) -> Result<impl MesocycleStore + Send + Sync + use<>, NextFailure> {
    rescheduling::gym(store, zone).await
}

/// Prescribing, against the plan as it now stands.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub async fn prescriber(
    store: &Store,
    zone: &OperatorZone,
) -> Result<
    Prescribing<
        impl application::ExerciseHistory + Send + Sync + use<>,
        Rescheduled<
            impl MesocycleStore + Send + Sync + use<>,
            impl PlanStore + Send + Sync + use<>,
        >,
        impl application::GenerationParameterStore + Send + Sync + use<>,
        impl PrescribedWorkoutStore + Send + Sync + use<>,
        impl application::PrescriptionLifecycle + Send + Sync + use<>,
    >,
    NextFailure,
> {
    Ok(Prescribing::new(PrescriptionPorts {
        history: store.exercise_history(),
        programmes: rescheduling::gym(store, zone).await?,
        parameters: store.parameters(),
        prescriptions: store.prescriptions(zone),
        lifecycle: store.gym_deliveries(),
    }))
}

/// Delivering what was issued, to a destination.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub async fn delivering<T: PrescriptionDestination + Sync>(
    store: &Store,
    zone: &OperatorZone,
    destination: T,
) -> Result<
    Delivering<
        impl PrescribedWorkoutStore + Send + Sync + use<T>,
        impl MesocycleStore + Send + Sync + use<T>,
        impl PrescriptionDeliveryStore + Send + Sync + use<T>,
        T,
    >,
    NextFailure,
> {
    Ok(Delivering::new(DeliveryPorts {
        prescriptions: store.prescriptions(zone),
        programmes: rescheduling::gym(store, zone).await?,
        deliveries: store.gym_deliveries(),
        destination,
    }))
}

/// Why a preview could not be rendered.
#[derive(Debug, thiserror::Error)]
pub enum PreviewFailure {
    #[error(transparent)]
    Next(#[from] NextFailure),
    #[error(transparent)]
    Delivery(#[from] DeliveryError),
    #[error("nothing was rendered")]
    NothingRendered,
}

/// The whole path except the two irreversible steps, and the body it would
/// have sent.
///
/// **The store it writes to is thrown away**, so a preview cannot leave a record
/// claiming a session was delivered — which would make the real delivery a
/// no-op and lose the session entirely. The rendering is the real one.
///
/// # Errors
///
/// [`PreviewFailure`] if the store cannot be read, nothing is issued for the
/// date, or nothing was rendered.
pub async fn preview(
    store: &Store,
    zone: &OperatorZone,
    date: Date,
) -> Result<(Delivery, String), PreviewFailure> {
    let destination = HevyRoutinePreview::new()?;

    let delivering = Delivering::new(DeliveryPorts {
        prescriptions: store.prescriptions(zone),
        programmes: rescheduling::gym(store, zone).await?,
        deliveries: ForgetfulDeliveries,
        destination: &destination,
    });
    let delivered = delivering.deliver(date).await?;

    let body = destination.body().ok_or(PreviewFailure::NothingRendered)?;
    Ok((delivered, body))
}

/// What a session did against what it was told.
#[must_use]
pub fn comparing(
    store: &Store,
    zone: &OperatorZone,
) -> Comparing<
    impl PrescribedWorkoutStore + Send + Sync + use<>,
    impl application::PerformedGymSessions + Send + Sync + use<>,
> {
    Comparing::new(ComparisonPorts {
        prescriptions: store.prescriptions(zone),
        workouts: store.canonical_gym(),
    })
}

/// A delivery store that remembers nothing.
///
/// Not a test double: it is what makes a preview safe to run against the real
/// store, because the one thing a preview must not do is record a delivery that
/// never happened.
struct ForgetfulDeliveries;

impl PrescriptionDeliveryStore for ForgetfulDeliveries {
    async fn reference_for(
        &self,
        _prescription: application::PrescribedWorkoutId,
        _destination: &application::DestinationName,
    ) -> Result<Option<application::DeliveryReference>, application::StoreError> {
        Ok(None)
    }

    /// **Nothing occupies anything, so a preview always renders a first
    /// delivery.** Answering otherwise would send the preview down the
    /// replacement path and have it print what a `PUT` would send — which is
    /// the same bytes, but aimed at a routine this run has no business naming.
    async fn occupying(
        &self,
        _date: Date,
        _destination: &application::DestinationName,
    ) -> Result<Option<application::Occupant>, application::StoreError> {
        Ok(None)
    }

    async fn record(
        &self,
        _prescription: application::PrescribedWorkoutId,
        _destination: &application::DestinationName,
        _reference: &application::DeliveryReference,
        _rendering: &application::RenderingDigest,
        _at: jiff::Timestamp,
    ) -> Result<(), application::StoreError> {
        Ok(())
    }

    async fn record_rendering(
        &self,
        _prescription: application::PrescribedWorkoutId,
        _destination: &application::DestinationName,
        _reference: &application::DeliveryReference,
        _rendering: &application::RenderingDigest,
        _at: jiff::Timestamp,
    ) -> Result<(), application::StoreError> {
        Ok(())
    }

    async fn hand_over(
        &self,
        _from: application::PrescribedWorkoutId,
        _to: application::PrescribedWorkoutId,
        _destination: &application::DestinationName,
        _reference: &application::DeliveryReference,
        _rendering: &application::RenderingDigest,
        _at: jiff::Timestamp,
    ) -> Result<(), application::StoreError> {
        Ok(())
    }

    /// **Nothing is answered, so nothing is kept.** The preview destination
    /// contacts no one and hands back no reply, so this is never reached with
    /// one — and it forgets on principle either way, which is what makes running
    /// a preview against the real store safe.
    async fn record_reply(
        &self,
        _prescription: application::PrescribedWorkoutId,
        _destination: &application::DestinationName,
        _reply: &application::DestinationReply,
        _at: jiff::Timestamp,
    ) -> Result<(), application::StoreError> {
        Ok(())
    }
}
