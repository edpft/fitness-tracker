//! Changing a slot's fill partway through a mesocycle, over this build's
//! store (#454).

use application::{FillChangeError, MesocycleInForce, fills::Refilling};
use domain::{
    gym::{Exercise, equipment::Specific, exercise::Implement},
    normalised::OperatorZone,
    prescription::SlotId,
    schedule::Relative,
};
use jiff::civil::Date;

use crate::store::Store;

fn refilling(
    store: &Store,
    zone: &OperatorZone,
) -> Refilling<
    impl application::MesocycleStore + Sync + use<>,
    impl application::FillChangeStore + Sync + use<>,
    impl application::EquipmentStore + Sync + use<>,
> {
    Refilling::new(
        store.gym_mesocycles(zone),
        store.fill_changes(zone),
        store.equipment(),
    )
}

/// The mesocycle in force on a date, with its changes.
///
/// # Errors
///
/// [`FillChangeError::Store`] if the store cannot be read.
pub async fn on(
    store: &Store,
    zone: &OperatorZone,
    date: Date,
) -> Result<Option<MesocycleInForce>, FillChangeError> {
    refilling(store, zone).on(date).await
}

/// Fill a slot from a date.
///
/// # Errors
///
/// See [`Refilling::change`].
pub async fn change(
    store: &Store,
    zone: &OperatorZone,
    slot: SlotId,
    intensity: Option<Relative>,
    from: Date,
    exercise: Exercise,
    assisted_by: Option<Specific>,
    loaded_by: Option<Implement>,
) -> Result<(), FillChangeError> {
    refilling(store, zone)
        .change(
            slot,
            intensity,
            from,
            exercise,
            assisted_by,
            loaded_by,
            jiff::Timestamp::now(),
        )
        .await
}
