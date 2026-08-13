//! The Hevy adapter.
//!
//! Everything specific to one source lives behind this module: its URL shape,
//! its header, its pagination cap, and the two places its published interface
//! disagrees with what it actually serves. Nothing above the port knows any of
//! it.

pub mod client;
pub mod page;
pub mod paging;
pub mod retry;

pub use client::{EVENTS_ENDPOINT, HevyWorkoutEvents};
pub use page::parse_page;
pub use paging::{PageCount, PageNumber};
pub use retry::RetryPolicy;
