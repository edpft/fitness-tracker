//! The core of the hexagon: entities, value objects, and the rules that
//! govern them.
//!
//! This crate depends on no workspace crate, and never on a framework,
//! database, or transport. If a change here forces a change in a web handler
//! or a SQL query, the dependency is pointing the wrong way.
//!
//! Data-type dependencies are fine — a timestamp, a hash. They describe
//! values rather than machinery.

pub mod gym;
pub mod landing;
mod newtype;
pub mod prescription;
pub mod schedule;
