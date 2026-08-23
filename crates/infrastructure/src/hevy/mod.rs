//! The Hevy adapter.
//!
//! Everything specific to one source lives behind this module: its URL shape,
//! its header, its pagination cap, and the two places its published interface
//! disagrees with what it actually serves. Nothing above the port knows any of
//! it.

pub mod client;
pub mod destination;
pub mod mapping;
pub mod page;
pub mod paging;
pub mod payload;
pub mod retry;
pub mod routine;
pub mod translate;
pub mod writable;

pub use client::{EVENTS_ENDPOINT, HevyWorkoutEvents};
pub use destination::{FOLDERS_ENDPOINT, HevyRoutinePreview, HevyRoutines, ROUTINES_ENDPOINT};
pub use mapping::{LoadReading, Mapped};
pub use page::parse_page;
pub use paging::{PageCount, PageNumber};
pub use retry::RetryPolicy;
pub use translate::HevyWorkoutTranslator;
pub use writable::{Unwritable, Writable, WrittenLoad, write_load};
