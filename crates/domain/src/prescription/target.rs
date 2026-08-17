//! What a set instructs.
//!
//! Volume, intensity and density — where intensity has three currencies and
//! exactly one is pinned. **The "prescribes nothing" case is absent by
//! construction**: every variant of [`Prescribed`] pins at least one axis, so a
//! set that instructs nothing cannot be built and nothing downstream checks for
//! one (§ 24).
//!
//! **Rest inverts against the performed side.** A performed `rest_after` is
//! optional because nobody recorded it; a prescribed one is optional because no
//! instruction was given. Same shape, opposite meaning — which is why these are
//! two types rather than one shared one, and why a shared one would quietly
//! answer "how long did you rest?" with "how long were you told to".

use std::fmt;

use crate::gym::{Duration, Load, Rir};

/// Why a target could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a range must span: its low bound is not below its high one")]
pub struct EmptyRange;

/// How much of the measure to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<M> {
    Exactly(M),
    Range { low: M, high: M },
}

impl<M: PartialOrd> Target<M> {
    /// A range, which must span.
    ///
    /// Equal bounds are [`Target::Exactly`] and there is no third state, so a
    /// caller cannot build a "range" that is secretly a point and then have two
    /// representations of one instruction to compare.
    ///
    /// # Errors
    ///
    /// [`EmptyRange`] if `low` is not below `high`.
    pub fn range(low: M, high: M) -> Result<Self, EmptyRange> {
        if low < high {
            Ok(Self::Range { low, high })
        } else {
            Err(EmptyRange)
        }
    }

    /// Does this performed measure satisfy the target?
    ///
    /// Asymmetric on purpose: a performed six satisfies a prescribed four-to-six
    /// and a prescribed six does not satisfy a performed four-to-six, because
    /// only one of the two is an instruction. The round trip in
    /// [`super::project`] depends on this being a relation rather than equality.
    pub fn satisfied_by(&self, measure: &M) -> bool {
        match self {
            Self::Exactly(target) => measure == target,
            Self::Range { low, high } => low <= measure && measure <= high,
        }
    }
}

impl<M: fmt::Display> fmt::Display for Target<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exactly(measure) => write!(f, "{measure}"),
            Self::Range { low, high } => write!(f, "{low}-{high}"),
        }
    }
}

/// One instruction, with exactly one axis left open.
///
/// The variants are not three styles of the same thing: each names which axis
/// the lifter is meant to resolve on the day, and a set with none of them open
/// is a set with nothing to discover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prescribed<M> {
    /// Load and measure pinned; effort is guidance. Warm-ups, back-offs, the
    /// primary's top set, and the first two sets of a double-progression slot.
    Fixed {
        load: Load,
        measure: Target<M>,
        effort: Option<Rir>,
    },
    /// Measure open; effort binds. The third set of the upper superset.
    ///
    /// `predicted` is typed apart from the prescription because a prediction the
    /// set overshoots is not an instruction the set exceeded.
    ToEffort {
        load: Load,
        effort: Rir,
        predicted: Option<Target<M>>,
    },
    /// Load open; effort binds; measure pinned. Programme v1's RPE cap.
    ///
    /// Reachable and currently unreached: no programme against the present
    /// schema issues one. It stays because variants are append-only — a v1
    /// programme still generating still needs it — and it is recorded here so
    /// nobody reads the enum as a menu of live options.
    Autoregulated { measure: Target<M>, effort: Rir },
}

impl<M> Prescribed<M> {
    /// The load, where the prescription pins one.
    pub const fn load(&self) -> Option<Load> {
        match self {
            Self::Fixed { load, .. } | Self::ToEffort { load, .. } => Some(*load),
            Self::Autoregulated { .. } => None,
        }
    }

    /// The measure, where the prescription pins one. A prediction is not a
    /// prescription and does not appear here.
    pub const fn measure(&self) -> Option<&Target<M>> {
        match self {
            Self::Fixed { measure, .. } | Self::Autoregulated { measure, .. } => Some(measure),
            Self::ToEffort { .. } => None,
        }
    }

    pub const fn effort(&self) -> Option<Rir> {
        match self {
            Self::Fixed { effort, .. } => *effort,
            Self::ToEffort { effort, .. } | Self::Autoregulated { effort, .. } => Some(*effort),
        }
    }

    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Fixed { .. } => "fixed",
            Self::ToEffort { .. } => "to_effort",
            Self::Autoregulated { .. } => "autoregulated",
        }
    }
}

/// A prescribed set: the instruction, plus the density axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrescribedSet<M> {
    pub prescription: Prescribed<M>,
    /// The instruction, naturally a range. Absent means none was given — not
    /// that the rest is unknown, which is what the performed side's absence
    /// means.
    pub rest_after: Option<Target<Duration>>,
    pub warmup: bool,
}

impl<M> PrescribedSet<M> {
    /// A working set with load and measure pinned and no effort guidance. The
    /// primary's top set, and the shape most of a session is.
    pub const fn fixed(load: Load, measure: Target<M>) -> Self {
        Self {
            prescription: Prescribed::Fixed {
                load,
                measure,
                effort: None,
            },
            rest_after: None,
            warmup: false,
        }
    }

    /// A ramp step. Same shape as a working set; the flag is what excludes it
    /// from a volume total.
    pub const fn warmup(load: Load, measure: Target<M>) -> Self {
        Self {
            prescription: Prescribed::Fixed {
                load,
                measure,
                effort: None,
            },
            rest_after: None,
            warmup: true,
        }
    }

    #[must_use]
    pub const fn with_effort(mut self, effort: Rir) -> Self {
        if let Prescribed::Fixed {
            effort: ref mut slot,
            ..
        } = self.prescription
        {
            *slot = Some(effort);
        }
        self
    }
}

impl<M: fmt::Display> fmt::Display for PrescribedSet<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.prescription {
            Prescribed::Fixed {
                load,
                measure,
                effort,
            } => {
                write!(f, "{load} × {measure}")?;
                if let Some(effort) = effort {
                    write!(f, ", {effort} in reserve")?;
                }
            }
            Prescribed::ToEffort {
                load,
                effort,
                predicted,
            } => {
                write!(f, "{load} × ")?;
                match predicted {
                    Some(measure) => write!(f, "~{measure}")?,
                    None => f.write_str("as many as")?,
                }
                write!(f, ", {effort} in reserve")?;
            }
            Prescribed::Autoregulated { measure, effort } => {
                write!(f, "{measure} at {effort} in reserve")?;
            }
        }
        if self.warmup {
            f.write_str(" (warm-up)")?;
        }
        Ok(())
    }
}
