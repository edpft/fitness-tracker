//! Driven (outbound) adapters: the implementations of the ports that
//! `application` declared.
//!
//! This is where a technology choice is allowed to show — sqlx, reqwest, a
//! filesystem. Swap this crate out and the domain and use cases do not move.

use std::{
    collections::BTreeMap,
    sync::{Mutex, PoisonError},
};

use application::{ItemRepository, RepositoryError};
use domain::{Item, ItemId};

/// An in-memory [`ItemRepository`]. Useful on day one and in tests; replace
/// it with a real store when you have one.
#[derive(Debug, Default)]
pub struct InMemoryItemRepository {
    items: Mutex<BTreeMap<ItemId, Item>>,
}

impl InMemoryItemRepository {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ItemRepository for InMemoryItemRepository {
    fn save(&self, item: Item) -> Result<(), RepositoryError> {
        let mut items = self.items.lock().unwrap_or_else(PoisonError::into_inner);
        items.insert(item.id(), item);
        Ok(())
    }

    fn find(&self, id: ItemId) -> Result<Option<Item>, RepositoryError> {
        let items = self.items.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(items.get(&id).cloned())
    }
}

#[cfg(test)]
mod tests {
    use application::ItemRepository;
    use domain::{Item, ItemId};

    use super::InMemoryItemRepository;

    #[test]
    fn round_trips_an_item() {
        let repository = InMemoryItemRepository::new();
        let item = Item::new(ItemId(1), "widget").expect("a named item is valid");

        repository.save(item.clone()).expect("in-memory store");

        assert_eq!(repository.find(ItemId(1)), Ok(Some(item)));
        assert_eq!(repository.find(ItemId(2)), Ok(None));
    }
}
