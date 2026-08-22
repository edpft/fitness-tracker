//! The number every primary load derives from.
//!
//! **The anchor is the block's starting 1RM, and it is fixed for the block's
//! duration.** What climbs is the ladder's position, expressed as a percentage
//! of it. An earlier design had the anchor itself advancing weekly, which
//! describes the same load sequence from the other end and cost the model its
//! endpoint: a value that climbs indefinitely gives a block nothing to be the
//! plan for.
//!
//! **Only a test replaces it, and a test ends a block.** So within a block it is
//! a constant, and nothing performed moves it — which is what makes the whole
//! prescription computable in advance from a duration and a starting 1RM. A
//! stall does not touch it: a stall is evidence that the plan was too ambitious,
//! not evidence about where the block started.

use std::fmt;

use jiff::civil::Date;

use crate::gym::Kg;

/// How the anchor was arrived at.
///
/// The three are not equally good and the difference matters six months later,
/// which is why it is carried rather than inferred. A tested anchor is a
/// measurement; an estimate is arithmetic over a set taken to failure; an
/// asserted one is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AnchorProvenance {
    /// Measured: a completed single under test, or a failure bounding one.
    Tested,
    /// Derived from a set taken to failure. Never from a submaximal set — a set
    /// left with repetitions in reserve says nothing about a maximum, whatever
    /// a formula returns for it.
    Estimated,
    /// Neither measured nor derived. A bootstrap.
    Asserted,
}

impl AnchorProvenance {
    /// The stable key. Persisted.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tested => "tested",
            Self::Estimated => "estimated",
            Self::Asserted => "asserted",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} does not name an anchor provenance")]
pub struct UnknownProvenance {
    value: String,
}

impl TryFrom<String> for AnchorProvenance {
    type Error = UnknownProvenance;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "tested" => Ok(Self::Tested),
            "estimated" => Ok(Self::Estimated),
            "asserted" => Ok(Self::Asserted),
            _ => Err(UnknownProvenance { value }),
        }
    }
}

impl fmt::Display for AnchorProvenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A starting 1RM, with how it was arrived at and when.
///
/// Provenance is a constructor argument rather than a setter, so an anchor that
/// exists is an anchor that knows where it came from.
///
/// **It carries the test's whole outcome, not just its best set.** A test that
/// found the ceiling completed one load and failed the one above it, and both
/// halves are evidence: the completed load is the maximum, and the failed load
/// is where the block opens. A test that failed nothing did not find the
/// ceiling, and the block opens one increment above what it did reach. See
/// `docs/decisions/0009-a-linear-block-opens-from-its-entry-test.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    load: Kg,
    failed: Option<Kg>,
    provenance: AnchorProvenance,
    from: Date,
}

impl Anchor {
    /// # Errors
    ///
    /// [`InvalidAnchor::NoLoad`] for no load — a block anchored at zero
    /// prescribes an empty bar every week — and
    /// [`InvalidAnchor::FailedBelowCompleted`] for a failed load at or below the
    /// completed one, which is not a test that found a ceiling.
    pub const fn new(
        load: Kg,
        failed: Option<Kg>,
        provenance: AnchorProvenance,
        from: Date,
    ) -> Result<Self, InvalidAnchor> {
        if load.as_grams() == 0 {
            return Err(InvalidAnchor::NoLoad);
        }
        if let Some(failed) = failed
            && failed.as_grams() <= load.as_grams()
        {
            return Err(InvalidAnchor::FailedBelowCompleted);
        }
        Ok(Self {
            load,
            failed,
            provenance,
            from,
        })
    }

    /// The heaviest single the test completed. This is the maximum.
    pub const fn load(self) -> Kg {
        self.load
    }

    /// What the test failed above it, if it found the ceiling.
    pub const fn failed(self) -> Option<Kg> {
        self.failed
    }

    pub const fn provenance(self) -> AnchorProvenance {
        self.provenance
    }

    pub const fn from(self) -> Date {
        self.from
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidAnchor {
    #[error("an anchor of no load is not a maximum")]
    NoLoad,
    #[error("a failed load at or below the completed one is not a ceiling")]
    FailedBelowCompleted,
}

impl fmt::Display for Anchor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.failed {
            Some(failed) => write!(
                f,
                "{}kg ({}, from {}, failed {}kg)",
                self.load, self.provenance, self.from, failed
            ),
            None => write!(
                f,
                "{}kg ({}, from {})",
                self.load, self.provenance, self.from
            ),
        }
    }
}

/// What a block's loads start from.
///
/// **The test that anchors it, and the opening where the block states one.**
/// These travel together because they answer one question between them — where
/// does this block's ladder begin — and because the answer is either/or: a
/// declared opening means the anchor's failed load feeds nothing.
///
/// Bundling them is not only tidiness. `Linear::new` and `Linear::rehydrate`
/// both take this, and passing an anchor without saying whether an opening
/// overrides it is the mistake the pair exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    anchor: Anchor,
    declared_opening: Option<Kg>,
}

impl Entry {
    /// Derive the opening from the test.
    pub const fn derived(anchor: Anchor) -> Self {
        Self {
            anchor,
            declared_opening: None,
        }
    }

    /// State the opening, leaving the test as evidence and nothing more.
    pub const fn declaring(anchor: Anchor, opening: Kg) -> Self {
        Self {
            anchor,
            declared_opening: Some(opening),
        }
    }

    /// Build from a stored pair, where the opening is a nullable column.
    pub const fn new(anchor: Anchor, declared_opening: Option<Kg>) -> Self {
        Self {
            anchor,
            declared_opening,
        }
    }

    pub const fn anchor(self) -> Anchor {
        self.anchor
    }

    /// The opening as authored, if it was authored at all. For reporting and
    /// for the store; the derivation goes through [`Self::opening`].
    pub const fn declared_opening(self) -> Option<Kg> {
        self.declared_opening
    }
}
