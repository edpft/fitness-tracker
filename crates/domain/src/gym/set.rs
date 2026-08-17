//! One performed set.
//!
//! The literature's definition: a group of repetitions performed consecutively
//! before stopping to rest, so the rest boundary *is* the set boundary. That is
//! why `rest_after` belongs here rather than between two of them.

use std::fmt;

use super::{intensity::Rir, load::Load, measure::Duration, outcome::Performed};

/// Working or warm-up, and nothing else.
///
/// Those are the two states the domain distinguishes, because volume metrics
/// need warm-ups excluded and nothing else about a set's kind changes what it
/// means.
///
/// A source's own kinds are not domain kinds, and mapping one onto these is the
/// adapter's job. There is no `Other` variant, so an unrecognised kind refuses
/// rather than defaulting — defaulting would file a set the source meant
/// something else by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SetKind {
    Working,
    Warmup,
}

impl SetKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Warmup => "warmup",
        }
    }
}

impl fmt::Display for SetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A set, counted in whatever its exercise is counted in.
///
/// Generic over the measure rather than holding a sum of them, so a
/// `Set<RepCount>` cannot reach a duration exercise. That is the partition
/// doing its work: nothing validates the pairing because nothing can build a
/// wrong one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Set<M> {
    pub load: Load,
    /// What became of it. A completed set carries its measure and a failed
    /// attempt carries none, so no arithmetic can take a quantity from a
    /// failure. The load is outside this, because a failed attempt is a load
    /// that was on the bar.
    pub outcome: Performed<M>,
    /// Absent where nothing recorded it. Not zero, and never carried forward
    /// from a neighbouring set.
    pub intensity: Option<Rir>,
    pub kind: SetKind,
    /// Resting two minutes rather than three between sets is a signal of
    /// progress, so this is a fact about a set even where nothing records it.
    ///
    /// Optional rather than absent from the model: a source that records none
    /// leaves partial data recorded as partial, which is not the same as the
    /// model having a hole in it. Reconstructing it from a linked routine would
    /// mean assuming every set took its prescribed rest, which is prescription
    /// masquerading as observation (§ 11).
    pub rest_after: Option<Duration>,
}

impl<M: fmt::Display> fmt::Display for Set<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} × {}", self.load, self.outcome)?;
        if let Some(intensity) = self.intensity {
            write!(f, " @ {intensity} in reserve")?;
        }
        if self.kind == SetKind::Warmup {
            f.write_str(" (warmup)")?;
        }
        Ok(())
    }
}
