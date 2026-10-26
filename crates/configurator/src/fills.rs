//! Changing a slot's fill partway through a mesocycle, over this build's
//! store (#454).

use application::{FillChangeError, MesocycleInForce, fills::Refilling};
use domain::{normalised::OperatorZone, prescription::FillChange};
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

/// Record a change to a slot's fill.
///
/// # Errors
///
/// See [`Refilling::change`].
pub async fn change(
    store: &Store,
    zone: &OperatorZone,
    change: FillChange,
) -> Result<(), FillChangeError> {
    refilling(store, zone).change(change).await
}
