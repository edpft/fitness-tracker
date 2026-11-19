//! Reporting drinking, an evening at a time, at whatever accuracy is known
//! (#873).
//!
//! **What each asks and refuses is here**, so the page reporting it has
//! nothing to implement again.

use domain::drinking::{Drink, Evening, OwnDrink, StandardServing};
use jiff::civil::Date;

use crate::{DrinkingAuthor, DrinkingError, DrinkingStore};

/// How many recent drinks are offered first.
const RECENT: usize = 5;

/// The drinks offered to count, in the order they are offered.
///
/// Each drink is offered once, in the first group that has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choices {
    /// Those counted on the latest evenings, latest first.
    pub recent: Vec<OwnDrink>,
    /// His own, by name.
    pub own: Vec<OwnDrink>,
    /// The NHS's standard servings.
    pub standard: Vec<OwnDrink>,
}

/// The use case, over the store that keeps the evenings.
pub struct Evenings<S> {
    store: S,
}

impl<S: DrinkingStore + DrinkingAuthor + Sync> Evenings<S> {
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Every evening with an entry, latest first.
    ///
    /// # Errors
    ///
    /// [`DrinkingError::Store`] if the store cannot be read.
    pub async fn evenings(&self) -> Result<Vec<Evening>, DrinkingError> {
        let mut evenings = self.store.evenings().await?;
        evenings.reverse();
        Ok(evenings)
    }

    /// Record an evening, replacing what was said about it before.
    ///
    /// # Errors
    ///
    /// [`DrinkingError::NotYet`] for an evening after `today`, and
    /// [`DrinkingError::Store`] if the store cannot be written.
    pub async fn record(&self, evening: Evening, today: Date) -> Result<Evening, DrinkingError> {
        if evening.date > today {
            return Err(DrinkingError::NotYet {
                evening: evening.date,
            });
        }
        self.store.record_evening(&evening).await?;
        Ok(evening)
    }

    /// Remove an evening's entry: no alcohol that evening after all.
    ///
    /// # Errors
    ///
    /// [`DrinkingError::Store`] if the store cannot be written.
    pub async fn remove(&self, date: Date) -> Result<(), DrinkingError> {
        Ok(self.store.remove_evening(date).await?)
    }

    /// Name one of his own drinks, replacing one of the same name.
    ///
    /// # Errors
    ///
    /// [`DrinkingError::Store`] if the store cannot be written.
    pub async fn name(&self, drink: OwnDrink) -> Result<OwnDrink, DrinkingError> {
        self.store.record_own_drink(&drink).await?;
        Ok(drink)
    }

    /// The drinks to offer: his recent ones, his own, then the NHS's.
    ///
    /// # Errors
    ///
    /// [`DrinkingError::Store`] if the store cannot be read.
    pub async fn choices(&self) -> Result<Choices, DrinkingError> {
        let evenings = self.store.evenings().await?;
        let own = self.store.own_drinks().await?;

        let mut offered: Vec<(String, Drink)> = Vec::new();
        let mut first = |drink: &OwnDrink| {
            let key = (drink.name.as_str().to_owned(), drink.drink);
            let new = !offered.contains(&key);
            if new {
                offered.push(key);
            }
            new
        };

        let mut recent = Vec::new();
        for drinks in evenings
            .iter()
            .rev()
            .flat_map(|evening| evening.drinking.counted())
        {
            if recent.len() == RECENT {
                break;
            }
            let drink = OwnDrink {
                name: drinks.name.clone(),
                drink: drinks.drink,
            };
            if first(&drink) {
                recent.push(drink);
            }
        }
        let own = own.into_iter().filter(|drink| first(drink)).collect();
        let standard = StandardServing::ALL
            .into_iter()
            .map(StandardServing::as_own)
            .filter(|drink| first(drink))
            .collect();
        Ok(Choices {
            recent,
            own,
            standard,
        })
    }
}
