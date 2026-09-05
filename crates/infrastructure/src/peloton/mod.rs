//! The Peloton adapter.
//!
//! Everything specific to Peloton lives behind this module. Nothing above the
//! port knows a `classId` exists.
//!
//! **Only the mapping so far.** Decision 0025 settled that Peloton should be a
//! source as well as a sink, and that a session should ideally be scheduled into
//! the operator's Peloton calendar — neither is built. What is here is the table
//! that says which class realises which session, which is what a prescription
//! needs in order to name where the ride is done.

pub mod auth;
pub mod class;
pub mod mapping;

pub use class::{ClassSession, PelotonClasses};
pub use mapping::{MappedSession, PEAK_YOUR_POWER_ZONES, PelotonClass};
