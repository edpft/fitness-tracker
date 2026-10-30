//! The crate's integration tests, as one binary (#648).
//!
//! Each file beside this one is a module of it, and the manifest turns off
//! cargo's discovery of the rest, so a file not named here is not compiled.
//! The `test-targets` check fails the gate on one.

mod attempts;
mod block;
mod body_weight;
mod calendar;
mod canonical_weigh_in;
mod canonicalise;
mod composition;
mod cycling;
mod device;
mod digest;
mod equipment;
mod ftp_test;
mod gym_entity;
mod gym_value_types;
mod heart_rate_per_zone;
mod holidays;
mod hrv;
mod ladder;
mod load_steps;
mod macrocycle;
mod manual_weigh_in;
mod mesocycle;
mod microcycle_state;
mod one_rep_max;
mod per_device;
mod plan;
mod planner;
mod prescription_entity;
mod prescription_value_types;
mod progression;
mod projection;
mod provider;
mod recomposition;
mod rest;
mod resting;
mod sbs;
mod schedule;
mod session_state;
mod value_types;
mod warmup;
