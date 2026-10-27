//! Stronger By Science's two-day intermediate squat routine.
//!
//! Two halves, and the split is the interesting part. [`chart`] is the published
//! table and the arithmetic that moves the maximum through it — the same for
//! everyone who runs the programme. [`mesocycle`] binds that chart to one lift,
//! one calendar and one opening maximum, which is all a plan states.
//! [`progression`] is the chart's values and the ramp, as the operator designs
//! them (#192).
//!
//! Decision 0024 has the reasoning and the provenance of every number.

pub mod chart;
pub mod mesocycle;
pub mod progression;

pub use chart::{
    CompletedSet, InvalidSbs, SbsChart, SbsDay, SbsSession, SbsWeek, WEEKS, achieved, advance,
    maximum_after, training_max_share, working_load,
};
pub use mesocycle::{GATING, Sbs};
pub use progression::SbsProgression;
