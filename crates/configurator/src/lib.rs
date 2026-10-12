//! The configurator: which adapters serve which port, for every frontend.
//!
//! **Cockburn's term for the part of a hexagonal system that connects adapters
//! to ports**, and the composition root both driving adapters share. `cli` and
//! `web` request, render and interact; what this build collects, where its
//! credentials and store are found, and which adapters a use case runs over
//! are decided here once, so neither frontend reimplements them and neither
//! depends on the other.

pub mod access;
pub mod catalogue;
pub mod config;
pub mod holidays;
pub mod next;
pub mod paths;
pub mod rides;
pub mod wiring;

/// What the operator stored, as a frontend passes it to a run.
pub use infrastructure::Credentials;
