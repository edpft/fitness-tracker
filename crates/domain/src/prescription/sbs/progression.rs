//! SBS as a progression the primary lift runs (#192).
//!
//! **Only what the publication leaves out.** SBS states its chart, and the
//! chart is SBS's: it is in code, as [`SbsChart::published`], and is not
//! authored (§ 14.1, amended 2026-10-27). It states no warm-up ramp and no rest,
//! so those are authored here and kept as dated versions; a plan is generated
//! from the version in force when it was planned.
//!
//! [`SbsChart::published`]: super::chart::SbsChart::published

use crate::{
    measure::Duration,
    prescription::{Target, warmup::WarmupStep},
    sequence::NonEmpty,
};

/// How long the primary rests after each kind of set SBS's chart issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbsRest {
    /// Between a percentage day's sets across.
    pub across: Target<Duration>,
    /// After a repetition-maximum day's set at the maximum.
    pub rep_max: Target<Duration>,
    /// After each back-off.
    pub back_off: Target<Duration>,
    /// Between a test's attempts.
    pub attempt: Target<Duration>,
}

/// One version of SBS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbsProgression {
    /// The ramp before a top set. A floor, as linear's is: a ramp toward a
    /// repetition maximum takes more repetitions than this.
    pub warmup: NonEmpty<WarmupStep>,
    pub rest: SbsRest,
}
