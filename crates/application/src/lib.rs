//! Use cases, and the ports through which they talk to the outside world.
//!
//! Depends only on `domain`. The ports are defined *here*, in terms the
//! application understands, and are implemented out in `infrastructure` and
//! `web` — that inversion is what keeps adapters swappable.

use std::fmt;

use domain::{InvalidItem, Item, ItemId};

/// A driven (outbound) port: something the application needs done for it.
///
/// `infrastructure` supplies the implementations — Postgres, an HTTP client,
/// an in-memory fake for tests.
pub trait ItemRepository {
    /// Persist an item, replacing any existing item with the same id.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if the underlying store is unreachable.
    fn save(&self, item: Item) -> Result<(), RepositoryError>;

    /// Fetch an item by id, if it exists.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if the underlying store is unreachable.
    fn find(&self, id: ItemId) -> Result<Option<Item>, RepositoryError>;
}

/// The application's view of a failing adapter: no SQL codes, no HTTP
/// statuses, nothing that would leak a specific technology inwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryError(pub String);

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "repository failure: {}", self.0)
    }
}

impl std::error::Error for RepositoryError {}

/// A driving (inbound) port: a use case the outside world can invoke.
///
/// `web` — or a CLI, or a message consumer — calls this and knows nothing
/// about how it is fulfilled.
pub trait CreateItem {
    /// Create and store a new item.
    ///
    /// # Errors
    ///
    /// Returns [`CreateItemError`] if the name is invalid or the store fails.
    fn create_item(&self, id: ItemId, name: &str) -> Result<Item, CreateItemError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateItemError {
    Invalid(InvalidItem),
    Repository(RepositoryError),
}

impl fmt::Display for CreateItemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(error) => write!(f, "{error}"),
            Self::Repository(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CreateItemError {}

impl From<InvalidItem> for CreateItemError {
    fn from(error: InvalidItem) -> Self {
        Self::Invalid(error)
    }
}

impl From<RepositoryError> for CreateItemError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

/// The use case itself: generic over its driven port, so the composition root
/// in `web` decides which adapter it gets.
pub struct ItemService<R> {
    repository: R,
}

impl<R> ItemService<R> {
    pub fn new(repository: R) -> Self {
        Self { repository }
    }
}

impl<R: ItemRepository> CreateItem for ItemService<R> {
    fn create_item(&self, id: ItemId, name: &str) -> Result<Item, CreateItemError> {
        let item = Item::new(id, name)?;
        self.repository.save(item.clone())?;
        Ok(item)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use domain::{InvalidItem, Item, ItemId};

    use super::{CreateItem, CreateItemError, ItemRepository, ItemService, RepositoryError};

    /// Testing a use case needs no database — just another adapter.
    #[derive(Default)]
    struct InMemoryItems(RefCell<Vec<Item>>);

    impl ItemRepository for InMemoryItems {
        fn save(&self, item: Item) -> Result<(), RepositoryError> {
            self.0.borrow_mut().push(item);
            Ok(())
        }

        fn find(&self, id: ItemId) -> Result<Option<Item>, RepositoryError> {
            Ok(self.0.borrow().iter().find(|item| item.id() == id).cloned())
        }
    }

    #[test]
    fn creates_and_stores_an_item() {
        let service = ItemService::new(InMemoryItems::default());

        let item = service
            .create_item(ItemId(1), "widget")
            .expect("a named item is valid");

        assert_eq!(item.name(), "widget");
        assert_eq!(
            service.repository.find(ItemId(1)).expect("in-memory store"),
            Some(item)
        );
    }

    #[test]
    fn refuses_to_store_an_invalid_item() {
        let service = ItemService::new(InMemoryItems::default());

        assert_eq!(
            service.create_item(ItemId(1), ""),
            Err(CreateItemError::Invalid(InvalidItem::EmptyName))
        );
        assert!(service.repository.0.borrow().is_empty());
    }
}
