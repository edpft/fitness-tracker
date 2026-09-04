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

use crate::gym::{Duration, Load, Rir, Spans};

/// How much of the measure to do.
///
/// **A range is a minimum and an extent, never two endpoints.** Two endpoints
/// are two independent values and nothing structural stops the second falling
/// below the first, so a pair can only *reject* an inversion at construction —
/// which § 24 says is the wrong place for it. A minimum with a strictly positive
/// extent cannot describe an inverted or empty range at all, so building one is
/// infallible and the error that used to guard it does not exist.
///
/// The extent's type comes from [`Spans`], because only some measures exclude
/// zero on their own: a rep count is already non-zero and is its own extent,
/// while a duration may legitimately be zero — a superset instructs exactly that
/// — and so has a positive counterpart standing beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<M: Spans> {
    Exactly(M),
    /// From `minimum`, spanning `extent`. The top is derived, never stored, so
    /// there are not two numbers here that could disagree.
    Range {
        minimum: M,
        extent: M::Extent,
    },
}

impl<M: Spans> Target<M> {
    /// A range opening at `minimum` and spanning `extent`.
    ///
    /// Infallible, which is the whole point: there is no argument to this that
    /// produces an empty range.
    pub const fn spanning(minimum: M, extent: M::Extent) -> Self {
        Self::Range { minimum, extent }
    }

    /// The least the target accepts.
    pub const fn minimum(self) -> M {
        match self {
            Self::Exactly(measure) => measure,
            Self::Range { minimum, .. } => minimum,
        }
    }

    /// The most the target accepts. Equal to the minimum for an exact target,
    /// which is what makes "the longest rest this instructs" one question rather
    /// than two.
    pub fn maximum(self) -> M {
        match self {
            Self::Exactly(measure) => measure,
            Self::Range { minimum, extent } => minimum.spanning(extent),
        }
    }

    /// Rebuild from a pair of bounds.
    ///
    /// **The only fallible way in, and it exists for reading back what
    /// something outside the domain wrote down** — a store holding two columns,
    /// a document naming two numbers. Inside the domain nothing needs it:
    /// [`Self::spanning`] cannot fail. `None` where the pair is not a range.
    pub fn between(low: M, high: M) -> Option<Self> {
        M::extent_between(low, high).map(|extent| Self::Range {
            minimum: low,
            extent,
        })
    }
}

impl<M: Spans + PartialOrd> Target<M> {
    /// Does this performed measure satisfy the target?
    ///
    /// Asymmetric on purpose: a performed six satisfies a prescribed four-to-six
    /// and a prescribed six does not satisfy a performed four-to-six, because
    /// only one of the two is an instruction. The round trip in
    /// [`super::project`] depends on this being a relation rather than equality.
    pub fn satisfied_by(&self, measure: &M) -> bool {
        match *self {
            Self::Exactly(target) => *measure == target,
            Self::Range { minimum, extent } => {
                minimum <= *measure && *measure <= minimum.spanning(extent)
            }
        }
    }
}

impl<M: Spans + fmt::Display> fmt::Display for Target<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Exactly(measure) => write!(f, "{measure}"),
            Self::Range { minimum, extent } => {
                write!(f, "{minimum}-{}", minimum.spanning(extent))
            }
        }
    }
}

/// One instruction, with exactly one axis left open.
///
/// The variants are not three styles of the same thing: each names which axis
/// the lifter is meant to resolve on the day, and a set with none of them open
/// is a set with nothing to discover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prescribed<M: Spans> {
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
    /// Load open; effort binds; measure pinned.
    ///
    /// Originally the RPE cap in the operator's own programme v1 — a version of
    /// their document, never a template of ours — and recorded here as reachable
    /// but unreached. It turned out to be exactly what a block's exit
    /// **test** is: one repetition, nothing in reserve, and the load is whatever
    /// the day allows. That is the only thing that issues one now.
    /// **`toward` is what the plan expects, not a cap.** A block's exit test has
    /// none: decision 0011 makes its target a function of where the progression
    /// stands, so it moves as the record does and a stored number would be stale
    /// the first time a session goes up. An SBS repetition-maximum day has one,
    /// because the chart derives it from the maximum current that week and it is
    /// fixed for the session — it is what the ramp was built toward, and going
    /// past it is still the outcome the day exists to produce.
    Autoregulated {
        measure: Target<M>,
        effort: Rir,
        toward: Option<Load>,
    },
}

impl<M: Spans> Prescribed<M> {
    /// The load, where the prescription pins one.
    pub const fn load(&self) -> Option<Load> {
        match self {
            Self::Fixed { load, .. } | Self::ToEffort { load, .. } => Some(*load),
            Self::Autoregulated { .. } => None,
        }
    }

    /// What an autoregulated set is expected to reach. Not a load the plan pins
    /// — [`load`](Self::load) stays `None` — but the number a destination that
    /// insists on one should be given.
    pub const fn toward(&self) -> Option<Load> {
        match self {
            Self::Autoregulated { toward, .. } => *toward,
            Self::Fixed { .. } | Self::ToEffort { .. } => None,
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
pub struct PrescribedSet<M: Spans> {
    pub prescription: Prescribed<M>,
    /// The instruction, naturally a range. Absent means none was given — not
    /// that the rest is unknown, which is what the performed side's absence
    /// means.
    pub rest_after: Option<Target<Duration>>,
    pub warmup: bool,
}

impl<M: Spans> PrescribedSet<M> {
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

    /// Load open, measure pinned, effort binding. A test.
    ///
    /// The variant recorded as reachable-but-unreached turns out to be exactly
    /// what a block's exit test is: work up to a single, nothing left in reserve,
    /// and the load is whatever the day allows.
    pub const fn autoregulated(measure: Target<M>, effort: Rir) -> Self {
        Self {
            prescription: Prescribed::Autoregulated {
                measure,
                effort,
                toward: None,
            },
            rest_after: None,
            warmup: false,
        }
    }

    /// What the plan expects an autoregulated set to reach. Nothing on any other
    /// variant, which already pin a load.
    #[must_use]
    pub const fn toward(mut self, target: Load) -> Self {
        if let Prescribed::Autoregulated {
            toward: ref mut slot,
            ..
        } = self.prescription
        {
            *slot = Some(target);
        }
        self
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

impl<M: Spans + fmt::Display> fmt::Display for PrescribedSet<M> {
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
            Prescribed::Autoregulated {
                measure,
                effort,
                toward,
            } => {
                match toward {
                    Some(load) => write!(f, "{measure} toward {load}")?,
                    None => write!(f, "{measure}")?,
                }
                write!(f, " at {effort} in reserve")?;
            }
        }
        if self.warmup {
            f.write_str(" (warm-up)")?;
        }
        Ok(())
    }
}
