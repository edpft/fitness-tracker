//! The Hevy adapter.
//!
//! Everything specific to one source lives behind this module: its URL shape,
//! its header, its pagination cap, and the two places its published interface
//! disagrees with what it actually serves. Nothing above the port knows any of
//! it.

pub mod client;
pub mod mapping;
pub mod page;
pub mod paging;
pub mod payload;
pub mod retry;
pub mod translate;

pub use client::{EVENTS_ENDPOINT, HevyWorkoutEvents};
pub use mapping::{LoadReading, Mapped};
pub use page::parse_page;
pub use paging::{PageCount, PageNumber};
pub use retry::RetryPolicy;
pub use translate::HevyWorkoutTranslator;
