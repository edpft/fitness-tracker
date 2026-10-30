//! The crate's integration tests, as one binary (#648).
//!
//! Each file beside this one is a module of it, and the manifest turns off
//! cargo's discovery of the rest, so a file not named here is not compiled.
//! The `test-targets` check fails the gate on one.

mod edge_cases;
mod extraction;
mod holding_ride;
mod next;
mod support;
mod watermark;
