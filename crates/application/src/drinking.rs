//! Reporting drinking, a day at a time, at whatever accuracy is known
//! (#873).
//!
//! **What each asks and refuses is here**, so the page reporting it has
//! nothing to implement again.

use domain::drinking::{Day, Drink, OwnDrink, StandardServing};
use jiff::civil::Date;

use crate::{DrinkingAuthor, DrinkingError, DrinkingStore};

/// How many recent drinks are offered first.
const RECENT: usize = 5;

/// The drinks offered to count, in the order they are offered.
///
/// Each drink is offered once, in the first group that has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choices {
    /// Those counted on the latest days, latest first.
    pub recent: Vec<OwnDrink>,
    /// His own, by name.
    pub own: Vec<OwnDrink>,
    /// The NHS's standard servings.
    pub standard: Vec<OwnDrink>,
}

/// The use case, over the store that keeps the days.
pub struct Days<S> {
    store: S,
}

impl<S: DrinkingStore + DrinkingAuthor + Sync> Days<S> {
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Every day with an entry, latest first.
    ///
    /// # Errors
    ///
    /// [`DrinkingError::Store`] if the store cannot be read.
    pub async fn days(&self) -> Result<Vec<Day>, DrinkingError> {
        let mut days = self.store.days().await?;
        days.reverse();
        Ok(days)
    }

    /// Record a day, replacing what was said about it before.
    ///
    /// # Errors
    ///
    /// [`DrinkingError::NotYet`] for a day after `today`, and
    /// [`DrinkingError::Store`] if the store cannot be written.
    pub async fn record(&self, day: Day, today: Date) -> Result<Day, DrinkingError> {
        if day.date > today {
            return Err(DrinkingError::NotYet { day: day.date });
        }
        self.store.record_day(&day).await?;
        Ok(day)
    }

    /// Remove a day's entry: no alcohol that day after all.
    ///
    /// # Errors
    ///
    /// [`DrinkingError::Store`] if the store cannot be written.
    pub async fn remove(&self, date: Date) -> Result<(), DrinkingError> {
        Ok(self.store.remove_day(date).await?)
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
        let days = self.store.days().await?;
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
        for drinks in days.iter().rev().flat_map(|day| day.drinking.counted()) {
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
