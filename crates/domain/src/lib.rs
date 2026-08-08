//! The core of the hexagon: entities, value objects, and the rules that
//! govern them.
//!
//! This crate depends on nothing — not on `application`, and never on a
//! framework, database, or transport. If a change here forces a change in a
//! web handler or a SQL query, the dependency is pointing the wrong way.

use std::fmt;

/// A stable identifier for an [`Item`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub u64);

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An example entity. Replace with your own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    id: ItemId,
    name: String,
}

impl Item {
    /// Construct an item, enforcing the invariants of the type.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidItem::EmptyName`] if `name` is blank.
    pub fn new(id: ItemId, name: impl Into<String>) -> Result<Self, InvalidItem> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(InvalidItem::EmptyName);
        }
        Ok(Self { id, name })
    }

    pub fn id(&self) -> ItemId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Why an [`Item`] could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidItem {
    EmptyName,
}

impl fmt::Display for InvalidItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "an item's name cannot be empty"),
        }
    }
}

impl std::error::Error for InvalidItem {}

#[cfg(test)]
mod tests {
    use super::{InvalidItem, Item, ItemId};

    #[test]
    fn builds_an_item_with_a_name() {
        let item = Item::new(ItemId(1), "widget").expect("a named item is valid");
        assert_eq!(item.name(), "widget");
    }

    #[test]
    fn rejects_a_blank_name() {
        assert_eq!(Item::new(ItemId(1), "   "), Err(InvalidItem::EmptyName));
    }
}
