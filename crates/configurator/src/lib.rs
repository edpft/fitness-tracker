//! The configurator: which adapters serve which port, for every frontend.
//!
//! **Cockburn's term for the part of a hexagonal system that connects adapters
//! to ports**, and the composition root both driving adapters share. `cli` and
//! `web` request, render and interact; what this build collects, where its
//! credentials and store are found, and which adapters a use case runs over
//! are decided here once, so neither frontend reimplements them and neither
//! depends on the other.

pub mod access;
pub mod analyses;
pub mod body_weight;
pub mod catalogue;
pub mod checks;
pub mod clock;
pub mod composition;
pub mod config;
pub mod cycling;
pub mod design;
pub mod devices;
pub mod drinking;
pub mod edits;
pub mod equipment;
pub mod fills;
pub mod ftp_tests;
pub mod graph;
pub mod held;
pub mod history;
pub mod holidays;
pub mod macrocycle;
pub mod next;
pub mod one_rep_max;
pub mod paths;
pub mod prescribing;
pub mod rescheduling;
pub mod rides;
pub mod scheduling;
pub mod setup;
pub mod store;
pub mod telemetry;
pub mod templates;
pub mod wiring;

/// What the operator stored, as a frontend passes it to a run.
pub use infrastructure::Credentials;
