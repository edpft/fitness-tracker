//! Reporting drinking, over this build's store (#873).

use application::{
    DrinkingError,
    drinking::{Choices, Evenings},
};
use domain::drinking::{Evening, OwnDrink};
use jiff::civil::Date;

use crate::store::Store;

fn evenings_of(
    store: &Store,
) -> Evenings<impl application::DrinkingStore + application::DrinkingAuthor + Sync + use<>> {
    Evenings::new(store.drinking())
}

/// Every evening with an entry, latest first.
///
/// # Errors
///
/// See [`Evenings::evenings`].
pub async fn evenings(store: &Store) -> Result<Vec<Evening>, DrinkingError> {
    evenings_of(store).evenings().await
}

/// Record an evening, replacing what was said about it before.
///
/// # Errors
///
/// See [`Evenings::record`].
pub async fn record(
    store: &Store,
    evening: Evening,
    today: Date,
) -> Result<Evening, DrinkingError> {
    evenings_of(store).record(evening, today).await
}

/// Remove an evening's entry.
///
/// # Errors
///
/// See [`Evenings::remove`].
pub async fn remove(store: &Store, date: Date) -> Result<(), DrinkingError> {
    evenings_of(store).remove(date).await
}

/// Name one of his own drinks.
///
/// # Errors
///
/// See [`Evenings::name`].
pub async fn name(store: &Store, drink: OwnDrink) -> Result<OwnDrink, DrinkingError> {
    evenings_of(store).name(drink).await
}

/// The drinks to offer.
///
/// # Errors
///
/// See [`Evenings::choices`].
pub async fn choices(store: &Store) -> Result<Choices, DrinkingError> {
    evenings_of(store).choices().await
}
