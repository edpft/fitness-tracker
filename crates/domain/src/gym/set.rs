//! One performed set.
//!
//! The literature's definition: a group of repetitions performed consecutively
//! before stopping to rest, so the rest boundary *is* the set boundary. That is
//! why `rest_after` belongs here rather than between two of them.

use std::fmt;

use super::{intensity::Rir, load::Load, measure::Duration};

/// Working or warm-up, and nothing else.
///
/// Those are the two states the domain distinguishes, because volume metrics
/// need warm-ups excluded and nothing else about a set's kind changes what it
/// means.
///
/// A source's own kinds are not domain kinds. Hevy's `failure` and `dropset`
/// are both working sets to the only question asked of the field, and a set
/// taken to failure is `Rir::Zero`, which is the reliable signal anyway — the
/// flag was used inconsistently and abandoned: 6 uses in 2024, 70 in 2025, one
/// in 1,335 sets in 2026, against 461 sets at the top of the scale of which
/// only 67 carry it. An unrecognised kind fails translation rather than
/// defaulting, which is why there is no `Other` variant to default to.
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
    pub measure: M,
    /// Absent where nothing recorded it. Not zero, and never carried forward
    /// from a neighbouring set.
    pub intensity: Option<Rir>,
    pub kind: SetKind,
    /// Resting two minutes rather than three between sets is a signal of
    /// progress, so this is a fact about a set even where nothing records it.
    ///
    /// Permanently absent from the Hevy adapter: its logged set carries no rest
    /// field and no per-set timestamps, and reconstructing it from a linked
    /// routine would mean assuming every set took its prescribed rest, which is
    /// prescription masquerading as observation (§ 11). Optional rather than
    /// missing, because that is partial data recorded as partial (§ 37) rather
    /// than a gap in the model.
    pub rest_after: Option<Duration>,
}

impl<M: fmt::Display> fmt::Display for Set<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} × {}", self.load, self.measure)?;
        if let Some(intensity) = self.intensity {
            write!(f, " @ {intensity} in reserve")?;
        }
        if self.kind == SetKind::Warmup {
            f.write_str(" (warmup)")?;
        }
        Ok(())
    }
}
