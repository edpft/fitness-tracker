//! The driving (inbound) adapter, and the composition root.
//!
//! This is the only place that names a concrete adapter: it picks the
//! implementations, injects them into the use cases, and translates between
//! the outside world and the driving ports.

use application::{CreateItem, ItemService};
use domain::ItemId;
use infrastructure::InMemoryItemRepository;

fn main() {
    // Composition root: choose the adapters here, once.
    let service = ItemService::new(InMemoryItemRepository::new());

    // In a real driving adapter this is a request handler, not a `main`.
    match service.create_item(ItemId(1), "widget") {
        Ok(item) => println!("created item {} named {}", item.id(), item.name()),
        Err(error) => eprintln!("could not create item: {error}"),
    }
}
