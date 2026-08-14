//! How close to failure a set was taken.
//!
//! Reps in reserve, on an ordinal scale of eight named positions. The positions
//! order and compare; they do not average or subtract, and the type is built so
//! that they cannot — "mean reps in reserve across the block" does not compile,
//! which is correct, because averaging an ordinal scale is not meaningful.
//!
//! `FourOrMore` is the last position rather than an open bound applied
//! generally: below four in reserve, precision is not claimed.

use std::fmt;

/// Why an intensity could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not a position on the reps-in-reserve scale")]
pub struct UnrecognisedIntensity {
    value: String,
}

impl UnrecognisedIntensity {
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Reps in reserve.
///
/// Ordered from hardest to easiest, which is what `Ord` reports: `Zero` is the
/// least and `FourOrMore` the greatest, so "harder than" is `<`. The derive is
/// on the declaration order deliberately — reordering the variants would
/// silently reorder the scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rir {
    Zero,
    ZeroOrOne,
    One,
    OneOrTwo,
    Two,
    TwoOrThree,
    Three,
    FourOrMore,
}

impl Rir {
    /// Every position, hardest first. The scale, enumerated once.
    pub const ALL: [Self; 8] = [
        Self::Zero,
        Self::ZeroOrOne,
        Self::One,
        Self::OneOrTwo,
        Self::Two,
        Self::TwoOrThree,
        Self::Three,
        Self::FourOrMore,
    ];

    /// The name a position is written and read back as.
    ///
    /// Not a number, and deliberately not the source's RPE either. Which RPE
    /// maps to which position is a fact about one adapter's scale, so it lives
    /// with that adapter.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zero => "0",
            Self::ZeroOrOne => "0-1",
            Self::One => "1",
            Self::OneOrTwo => "1-2",
            Self::Two => "2",
            Self::TwoOrThree => "2-3",
            Self::Three => "3",
            Self::FourOrMore => "4+",
        }
    }
}

impl TryFrom<String> for Rir {
    type Error = UnrecognisedIntensity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|position| position.as_str() == value)
            .ok_or(UnrecognisedIntensity { value })
    }
}

impl fmt::Display for Rir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

crate::newtype::from_str_via_string!(Rir, UnrecognisedIntensity);
