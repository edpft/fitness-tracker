//! What became of a set.
//!
//! A load can be put on the bar and not moved, and that is neither a set of
//! zero repetitions nor an absence of a set. It is a third thing, and the
//! prescribed side needs it: the gate in `docs/primary-lift-progression.md`
//! retreats on evidence, a stall is two failures at one load, and a failure the
//! normalised layer will not represent is a stall the programme cannot see.
//!
//! **Zero stays unrepresentable as a count.** [`RepCount`] keeps its
//! `NonZeroU32`; a failure is a different variant rather than a small number.
//! That is what makes "a failure contributes nothing to a total" a property of
//! the type instead of a rule every caller has to remember — there is no arm in
//! which a failure yields a quantity, so no arithmetic can extract one.
//!
//! [`RepCount`]: super::measure::RepCount

use std::fmt;

/// A set's outcome, generic over the measure like the set itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Performed<M> {
    /// Done, and counted in whatever the exercise is counted in.
    Completed(M),
    /// Attempted and not completed. Carries nothing, by construction.
    Failed,
}

impl<M> Performed<M> {
    /// The measure, where there is one.
    ///
    /// The only way to a quantity, and it is fallible — which is the point. A
    /// caller summing volume writes `filter_map(Performed::completed)` and the
    /// failure drops out; a caller that wanted to count failures had to ask a
    /// different question anyway.
    pub const fn completed(&self) -> Option<&M> {
        match self {
            Self::Completed(measure) => Some(measure),
            Self::Failed => None,
        }
    }

    pub const fn is_failed(&self) -> bool {
        matches!(self, Self::Failed)
    }

    /// The stable key. Persisted, so it outlives a rename.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Completed(_) => "completed",
            Self::Failed => "failed",
        }
    }
}

impl<M: fmt::Display> fmt::Display for Performed<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Completed(measure) => write!(f, "{measure}"),
            Self::Failed => f.write_str("failed"),
        }
    }
}
