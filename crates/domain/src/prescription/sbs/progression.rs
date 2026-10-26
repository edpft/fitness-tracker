//! SBS as a progression the primary lift runs (#192).
//!
//! **Its ramp and its chart**, both set when the programme is designed and kept
//! as dated versions (§ 14.1, amended 2026-10-26). A plan is generated from the
//! version in force when it was planned.

use crate::{prescription::warmup::WarmupStep, sequence::NonEmpty};

use super::chart::SbsChart;

/// One version of SBS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbsProgression {
    /// The ramp before a top set. A floor, as linear's is: a ramp toward a
    /// repetition maximum takes more repetitions than this.
    pub warmup: NonEmpty<WarmupStep>,
    pub chart: SbsChart,
}
