//! Stronger By Science's two-day intermediate squat routine.
//!
//! Two halves, and the split is the interesting part. [`chart`] is the published
//! table and the arithmetic that moves the maximum through it — the same for
//! everyone who runs the programme. [`programme`] binds that chart to one lift,
//! one calendar and one opening maximum, which is all an operator authors.
//!
//! Decision 0024 has the reasoning and the provenance of every number.

pub mod chart;
pub mod programme;

pub use chart::{
    InvalidSbs, SbsDay, SbsSession, WEEKS, advance, day, maximum_after, training_max_share,
    working_load,
};
pub use programme::Sbs;
