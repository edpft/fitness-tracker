//! The number a week's loads are shares of, and where it came from.
//!
//! **An anchor belongs to a microcycle, not to a mesocycle.** It changes week
//! to week: every week that measures something leaves a new one behind, and the
//! week after it programmes from that. SBS has always worked this way — each
//! repetition-maximum day resets what the following week is a share of — and
//! what changed is that this is now the model rather than one template's
//! special case. A number authored onto a mesocycle and frozen for its duration
//! could only ever describe the first week of it.
//!
//! **So almost none of them is stored.** An anchor after the first is a
//! function of a test that happened, read off the record when a session is
//! asked for — which is what lets a plan hold three cycles without stating what
//! the second and third open from, and what lets the same authored cycle be run
//! again in January against January's record.
//!
//! **The exception is the one at the front.** An opening entry test has nothing
//! before it to read, so its anchor is [`AnchorProvenance::Asserted`]: a number
//! the operator states, because no test has produced one yet. That is the only
//! anchor anything authors, and the provenance is the whole difference between
//! it and every anchor that follows.

use std::fmt;

use jiff::civil::Date;

use crate::measure::Kg;

/// How a maximum was arrived at.
///
/// The three are not equally good and the difference matters six months later,
/// which is why it is carried rather than inferred. A tested maximum is a
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
#[error("{value:?} does not name a provenance")]
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

/// A lift's maximum, with how it was arrived at and the day it took effect.
///
/// The provenance is a constructor argument rather than a setter, so an anchor
/// that exists is one that knows where it came from.
///
/// **It carries the test's whole outcome, not just its best set.** A test that
/// found the ceiling completed one load and failed the one above it, and both
/// halves are evidence: the completed load is the maximum, and the failed load
/// is what a block opening derives from. A test that failed nothing did not find
/// the ceiling, and a block opening from it starts one increment above what it
/// did reach.
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
    /// [`InvalidAnchor::NoLoad`] for no load — a programme running off zero
    /// prescribes an empty bar every week — and
    /// [`InvalidAnchor::FailedBelowCompleted`] for a failed load at or below
    /// the completed one, which is not a test that found a ceiling.
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

    /// The heaviest single that went up. This is the maximum.
    pub const fn load(self) -> Kg {
        self.load
    }

    /// What was failed above it, if the ceiling was found.
    pub const fn failed(self) -> Option<Kg> {
        self.failed
    }

    pub const fn provenance(self) -> AnchorProvenance {
        self.provenance
    }

    /// The day it took effect: the day it was measured or stated for.
    pub const fn from(self) -> Date {
        self.from
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidAnchor {
    #[error("a maximum of no load is not a measurement")]
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

/// The one in force on a date: the latest that took effect at or before it.
///
/// **Later wins, whatever its provenance.** A stated number that postdates a
/// test supersedes it, because the operator stating one after a test is
/// correcting the record rather than guessing at it. Nothing is discarded for
/// being old: a maximum from last year still applies if nothing has superseded
/// it, and a lift that has not been trained for a year is a fact about the
/// training rather than a reason to refuse a number.
#[must_use]
pub fn in_force(anchors: &[Anchor], on: Date) -> Option<Anchor> {
    anchors
        .iter()
        .filter(|anchor| anchor.from <= on)
        .max_by_key(|anchor| anchor.from)
        .copied()
}
