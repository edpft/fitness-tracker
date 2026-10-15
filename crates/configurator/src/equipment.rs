//! The equipment the operator trains on, over this build's store (#450).

use application::{EquipmentError, equipment::Equipping};
use domain::{
    gym::equipment::{EquipmentFacts, FactId, Statement},
    normalised::OperatorZone,
};
use jiff::civil::Date;

use crate::store::Store;

fn equipping(store: &Store) -> Equipping<impl application::EquipmentStore + Sync + use<>> {
    Equipping::new(store.equipment())
}

/// Everything now known.
///
/// # Errors
///
/// [`EquipmentError::Store`] if the store cannot be read.
pub async fn facts(store: &Store) -> Result<EquipmentFacts, EquipmentError> {
    equipping(store).facts().await
}

/// State what something is from a date, or always when `from` is `None`.
///
/// # Errors
///
/// See [`Equipping::declare`].
pub async fn declare(
    store: &Store,
    statement: &Statement,
    from: Option<Date>,
) -> Result<FactId, EquipmentError> {
    equipping(store)
        .declare(statement, from, jiff::Timestamp::now())
        .await
}

/// Say what a fact should have said.
///
/// # Errors
///
/// See [`Equipping::correct`].
pub async fn correct(
    store: &Store,
    fact: FactId,
    statement: &Statement,
) -> Result<FactId, EquipmentError> {
    equipping(store)
        .correct(fact, statement, jiff::Timestamp::now())
        .await
}

/// Today, where the operator trains: what "from today" means on the page.
#[must_use]
pub fn today(zone: &OperatorZone) -> Date {
    jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date()
}
