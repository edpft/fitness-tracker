//! Reporting drinking, over this build's store (#873).

use application::{
    DrinkingError,
    drinking::{Choices, Days},
};
use domain::drinking::{Day, OwnDrink};
use jiff::civil::Date;

use crate::store::Store;

fn days_of(
    store: &Store,
) -> Days<impl application::DrinkingStore + application::DrinkingAuthor + Sync + use<>> {
    Days::new(store.drinking())
}

/// Every day with an entry, latest first.
///
/// # Errors
///
/// See [`Days::days`].
pub async fn days(store: &Store) -> Result<Vec<Day>, DrinkingError> {
    days_of(store).days().await
}

/// Record a day, replacing what was said about it before.
///
/// # Errors
///
/// See [`Days::record`].
pub async fn record(store: &Store, day: Day, today: Date) -> Result<Day, DrinkingError> {
    days_of(store).record(day, today).await
}

/// Remove a day's entry.
///
/// # Errors
///
/// See [`Days::remove`].
pub async fn remove(store: &Store, date: Date) -> Result<(), DrinkingError> {
    days_of(store).remove(date).await
}

/// Name one of his own drinks.
///
/// # Errors
///
/// See [`Days::name`].
pub async fn name(store: &Store, drink: OwnDrink) -> Result<OwnDrink, DrinkingError> {
    days_of(store).name(drink).await
}

/// The drinks to offer.
///
/// # Errors
///
/// See [`Days::choices`].
pub async fn choices(store: &Store) -> Result<Choices, DrinkingError> {
    days_of(store).choices().await
}
