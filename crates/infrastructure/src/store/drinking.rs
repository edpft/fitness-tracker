//! What the operator has reported drinking, in SQLite (#873).
//!
//! **Kept like an illness**: one row per evening, and recording an evening
//! again replaces it. When it was last written is kept beside it, which is
//! where the evenings reported as they happened can be told from those
//! recalled afterwards.

use std::num::{NonZeroU8, NonZeroU16};

use application::{DrinkingAuthor, DrinkingStore, StoreError};
use domain::{
    drinking::{Drink, DrinkName, Drinking, Drinks, Evening, Millilitres, OwnDrink, Strength},
    sequence::NonEmpty,
};
use jiff::{Timestamp, civil::Date};

use super::{authoring::Authoring, corrupt, store_error};

/// The evenings and his own drinks, in SQLite.
pub struct SqliteDrinkingStore {
    store: Authoring,
}

impl SqliteDrinkingStore {
    pub const fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            store: Authoring::new(pool),
        }
    }
}

const UNKNOWN: &str = "unknown";
const AT_LEAST: &str = "at_least";
const EXACTLY: &str = "exactly";

fn drink_of(millilitres: i64, strength: i64) -> Result<Drink, StoreError> {
    let volume = u16::try_from(millilitres)
        .ok()
        .and_then(NonZeroU16::new)
        .map(Millilitres::new)
        .ok_or_else(|| corrupt(&format!("{millilitres} ml is not a volume")))?;
    let strength = u16::try_from(strength)
        .ok()
        .and_then(NonZeroU16::new)
        .ok_or_else(|| corrupt(&format!("{strength} tenths is not a strength")))
        .and_then(|tenths| Strength::from_tenths(tenths).map_err(|error| corrupt(&error)))?;
    Ok(Drink { volume, strength })
}

fn name_of(name: String) -> Result<DrinkName, StoreError> {
    DrinkName::try_from(name).map_err(|error| corrupt(&error))
}

impl DrinkingStore for SqliteDrinkingStore {
    async fn evenings(&self) -> Result<Vec<Evening>, StoreError> {
        let mut reading = self.store.reading().await?;
        let evenings = sqlx::query!(
            r"
            SELECT evening, amount
            FROM drinking_evening
            ORDER BY evening
            "
        )
        .fetch_all(&mut *reading)
        .await
        .map_err(|error| store_error(&error))?;
        let drinks = sqlx::query!(
            r"
            SELECT evening, count, name, millilitres, strength
            FROM drinking_drinks
            ORDER BY evening, position
            "
        )
        .fetch_all(&mut *reading)
        .await
        .map_err(|error| store_error(&error))?;

        evenings
            .into_iter()
            .map(|row| {
                let date: Date = row
                    .evening
                    .parse()
                    .map_err(|_| corrupt(&format!("{:?} is not a date", row.evening)))?;
                let counted = drinks
                    .iter()
                    .filter(|drink| drink.evening == row.evening)
                    .map(|drink| {
                        Ok(Drinks {
                            count: u8::try_from(drink.count)
                                .ok()
                                .and_then(NonZeroU8::new)
                                .ok_or_else(|| {
                                    corrupt(&format!("{} is not a count", drink.count))
                                })?,
                            name: name_of(drink.name.clone())?,
                            drink: drink_of(drink.millilitres, drink.strength)?,
                        })
                    })
                    .collect::<Result<Vec<_>, StoreError>>()?;
                let counted = || {
                    NonEmpty::new(counted.clone())
                        .map_err(|_| corrupt(&format!("{date} counted no drinks")))
                };
                let drinking = match row.amount.as_str() {
                    UNKNOWN => Drinking::AmountUnknown,
                    AT_LEAST => Drinking::AtLeast(counted()?),
                    EXACTLY => Drinking::Exactly(counted()?),
                    other => return Err(corrupt(&format!("{other:?} is not an amount"))),
                };
                Ok(Evening { date, drinking })
            })
            .collect()
    }

    async fn own_drinks(&self) -> Result<Vec<OwnDrink>, StoreError> {
        let mut reading = self.store.reading().await?;
        sqlx::query!(
            r"
            SELECT name, millilitres, strength
            FROM own_drink
            ORDER BY name
            "
        )
        .fetch_all(&mut *reading)
        .await
        .map_err(|error| store_error(&error))?
        .into_iter()
        .map(|row| {
            Ok(OwnDrink {
                name: name_of(row.name)?,
                drink: drink_of(row.millilitres, row.strength)?,
            })
        })
        .collect()
    }
}

impl DrinkingAuthor for SqliteDrinkingStore {
    async fn record_evening(&self, evening: &Evening) -> Result<(), StoreError> {
        let authored_at = Timestamp::now().to_string();
        let date = evening.date.to_string();
        let amount = match evening.drinking {
            Drinking::AmountUnknown => UNKNOWN,
            Drinking::AtLeast(_) => AT_LEAST,
            Drinking::Exactly(_) => EXACTLY,
        };
        let mut tx = self.store.writing::<Self>().await?;
        sqlx::query!("DELETE FROM drinking_evening WHERE evening = ?", date)
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!(
            r"
            INSERT INTO drinking_evening (evening, authored_at, amount)
            VALUES (?, ?, ?)
            ",
            date,
            authored_at,
            amount
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
        for (position, drinks) in (0_i64..).zip(evening.drinking.counted()) {
            let count = drinks.count.get();
            let name = drinks.name.as_str();
            let millilitres = drinks.drink.volume.get();
            let strength = drinks.drink.strength.as_tenths();
            sqlx::query!(
                r"
                INSERT INTO drinking_drinks (evening, position, count, name, millilitres, strength)
                VALUES (?, ?, ?, ?, ?, ?)
                ",
                date,
                position,
                count,
                name,
                millilitres,
                strength
            )
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        }
        tx.commit().await
    }

    async fn remove_evening(&self, date: Date) -> Result<(), StoreError> {
        let date = date.to_string();
        let mut tx = self.store.writing::<Self>().await?;
        sqlx::query!("DELETE FROM drinking_evening WHERE evening = ?", date)
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        tx.commit().await
    }

    async fn record_own_drink(&self, drink: &OwnDrink) -> Result<(), StoreError> {
        let authored_at = Timestamp::now().to_string();
        let name = drink.name.as_str();
        let millilitres = drink.drink.volume.get();
        let strength = drink.drink.strength.as_tenths();
        let mut tx = self.store.writing::<Self>().await?;
        sqlx::query!(
            r"
            INSERT INTO own_drink (name, authored_at, millilitres, strength)
            VALUES (?, ?, ?, ?)
            ON CONFLICT (name) DO UPDATE
                SET authored_at = excluded.authored_at,
                    millilitres = excluded.millilitres,
                    strength    = excluded.strength
            ",
            name,
            authored_at,
            millilitres,
            strength
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
        tx.commit().await
    }
}

/// What the operator has reported drinking (#873).
impl application::Input for SqliteDrinkingStore {
    fn reads() -> application::Reads {
        application::Reads::of([application::Node::DRINKING])
    }
}

impl application::NotRaw for SqliteDrinkingStore {}

impl application::Authors for SqliteDrinkingStore {
    fn authors() -> std::collections::BTreeSet<application::Node> {
        [application::Node::DRINKING].into()
    }
}
