//! Declaring, changing and correcting what the equipment is (issue #450).
//!
//! **Three writes, two shapes.** Declaring and changing are one act: a fact
//! true from a date, which for something new starts its first period and for
//! something known starts its next. Correcting is the other: the same period,
//! with what it should have said, superseding what it did say. The operator,
//! 2026-10-15: *"declare facts about the world, correct them if they are wrong,
//! and change them if they change."*

use domain::gym::equipment::{EquipmentFacts, FactId, Statement};
use jiff::{Timestamp, civil::Date};

use crate::{EquipmentError, EquipmentStore};

/// The use case, over the store that keeps the facts.
pub struct Equipping<S> {
    store: S,
}

impl<S: EquipmentStore + Sync> Equipping<S> {
    pub const fn new(store: S) -> Self {
        Self { store }
    }

    /// Everything now known.
    ///
    /// # Errors
    ///
    /// [`EquipmentError::Store`] if the store cannot be read.
    pub async fn facts(&self) -> Result<EquipmentFacts, EquipmentError> {
        Ok(self.store.facts().await?)
    }

    /// State what something is from a date: new equipment, or a change to what
    /// is known. `None` is since before anything was recorded.
    ///
    /// # Errors
    ///
    /// [`EquipmentError::AlreadyStated`] if a period already starts then, which
    /// is a correction rather than a change.
    pub async fn declare(
        &self,
        statement: &Statement,
        from: Option<Date>,
        at: Timestamp,
    ) -> Result<FactId, EquipmentError> {
        let equipment = statement.equipment();
        if self.store.facts().await?.starts(&equipment, from) {
            return Err(EquipmentError::AlreadyStated {
                equipment: equipment.to_string(),
                from: from.map_or_else(|| "always".to_owned(), |date| format!("from {date}")),
            });
        }
        Ok(self.store.record(statement, from, at, None).await?)
    }

    /// Say what a fact should have said. Its period is unchanged.
    ///
    /// # Errors
    ///
    /// [`EquipmentError::NoSuchFact`] for a fact not now believed, and
    /// [`EquipmentError::NotTheSameEquipment`] for a statement about something
    /// else.
    pub async fn correct(
        &self,
        fact: FactId,
        statement: &Statement,
        at: Timestamp,
    ) -> Result<FactId, EquipmentError> {
        let Some((was, from)) = self.store.facts().await?.find(fact) else {
            return Err(EquipmentError::NoSuchFact { id: fact.get() });
        };
        let now = statement.equipment();
        if was != now {
            return Err(EquipmentError::NotTheSameEquipment {
                id: fact.get(),
                was: was.to_string(),
                now: now.to_string(),
            });
        }
        Ok(self.store.record(statement, from, at, Some(fact)).await?)
    }
}
