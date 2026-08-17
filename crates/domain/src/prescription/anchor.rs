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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    load: Kg,
    provenance: AnchorProvenance,
    from: Date,
}

impl Anchor {
    /// # Errors
    ///
    /// [`InvalidAnchor`] for no load. A block anchored at zero prescribes an
    /// empty bar every week.
    pub const fn new(
        load: Kg,
        provenance: AnchorProvenance,
        from: Date,
    ) -> Result<Self, InvalidAnchor> {
        if load.as_grams() == 0 {
            return Err(InvalidAnchor);
        }
        Ok(Self {
            load,
            provenance,
            from,
        })
    }

    pub const fn load(self) -> Kg {
        self.load
    }

    pub const fn provenance(self) -> AnchorProvenance {
        self.provenance
    }

    pub const fn from(self) -> Date {
        self.from
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("an anchor of no load is not a maximum")]
pub struct InvalidAnchor;

impl fmt::Display for Anchor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}kg ({}, from {})",
            self.load, self.provenance, self.from
        )
    }
}
