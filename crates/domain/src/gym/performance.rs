//! One exercise as it was performed in one session, as the record's readers
//! read it (#817).
//!
//! **Each set as the one thing it is.** The operator,
//! 2026-11-15: *"a performed set is either a completed warm up, a completed
//! working, or a failed working set, and only completed working sets have
//! perceived effort."* And a completed working set whose effort the source
//! left blank is a fourth, [`PerformedSet::Unrated`], until the edit overlay
//! states it (operator, 2026-11-15). A canonical set that leaves its load,
//! count or kind unstated is not a [`PerformedSet`], and stays in landing rather
//! than reaching a reader.

use crate::{
    canonical::Occurred,
    gym::{Load, Rir, exercise::RepsExercise},
    measure::RepCount,
    sequence::NonEmpty,
};

/// One set, as one of the three things a set can be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerformedSet {
    /// Below what the session asked of the lift, so no effort is stated.
    Warmup { load: Load, reps: RepCount },
    Working {
        load: Load,
        reps: RepCount,
        rir: Rir,
    },
    /// A completed working set whose effort the record does not state. Nothing
    /// that needs an effort reads one off it.
    Unrated { load: Load, reps: RepCount },
    /// A working set whose first repetition was not completed: a load on the
    /// bar and nothing else.
    Failed { load: Load },
}

impl PerformedSet {
    pub const fn load(&self) -> Load {
        match self {
            Self::Warmup { load, .. }
            | Self::Working { load, .. }
            | Self::Unrated { load, .. }
            | Self::Failed { load } => *load,
        }
    }

    /// The repetitions completed, `None` for a failed attempt.
    pub const fn reps(&self) -> Option<RepCount> {
        match self {
            Self::Warmup { reps, .. } | Self::Working { reps, .. } | Self::Unrated { reps, .. } => {
                Some(*reps)
            }
            Self::Failed { .. } => None,
        }
    }

    /// The load, where it is a mass on the bar rather than one taken off.
    pub const fn absolute(&self) -> Option<crate::measure::Kg> {
        match self.load() {
            Load::Absolute(load) => Some(load),
            Load::Relative(_) => None,
        }
    }

    pub const fn is_working(&self) -> bool {
        !matches!(self, Self::Warmup { .. })
    }
}

/// One exercise performed in one session, its sets in the order performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Performance {
    pub occurred: Occurred,
    pub exercise: RepsExercise,
    pub sets: NonEmpty<PerformedSet>,
}

impl Performance {
    /// The day it was performed on, in the zone it was performed in.
    pub fn on(&self) -> jiff::civil::Date {
        self.occurred.day()
    }
}
