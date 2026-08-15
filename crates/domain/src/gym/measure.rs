//! What a set is counted in.
//!
//! Three measures, because there are three things you can count: repetitions,
//! elapsed time, and ground covered.
//!
//! An exercise's measure is fixed by which vocabulary it belongs to, so a set
//! and its exercise cannot disagree and nothing needs validating.

use std::{fmt, num::NonZeroU32};

/// Why a quantity could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidQuantity {
    #[error("{value:?} is not a whole number of {unit}")]
    NotANumber { unit: &'static str, value: String },
    #[error("{unit} cannot be negative, and {value:?} is")]
    Negative { unit: &'static str, value: String },
    #[error("a set of zero reps is not a set")]
    ZeroReps,
}

/// How many times the movement was performed.
///
/// Zero is unrepresentable rather than rejected. A rep attempted and missed is a
/// real event and is not a set, so it needs an *attempt*, which belongs with
/// prescribed-versus-performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepCount(NonZeroU32);

impl RepCount {
    pub const fn as_u32(self) -> u32 {
        self.0.get()
    }

    /// # Errors
    ///
    /// [`InvalidQuantity::ZeroReps`] for zero.
    pub const fn new(reps: u32) -> Result<Self, InvalidQuantity> {
        match NonZeroU32::new(reps) {
            Some(reps) => Ok(Self(reps)),
            None => Err(InvalidQuantity::ZeroReps),
        }
    }
}

impl From<NonZeroU32> for RepCount {
    fn from(reps: NonZeroU32) -> Self {
        Self(reps)
    }
}

impl TryFrom<String> for RepCount {
    type Error = InvalidQuantity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let reps: u32 = value.parse().map_err(|_| InvalidQuantity::NotANumber {
            unit: "reps",
            value,
        })?;
        Self::new(reps)
    }
}

impl fmt::Display for RepCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Elapsed time, in seconds.
///
/// Its own type rather than `jiff::Span`, because what a set records is a scalar
/// count of seconds and a span carries calendar units that would have to be
/// normalised away every time two were compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Duration(u64);

impl Duration {
    pub const ZERO: Self = Self(0);

    pub const fn as_seconds(self) -> u64 {
        self.0
    }

    pub const fn from_seconds(seconds: u64) -> Self {
        Self(seconds)
    }
}

impl TryFrom<String> for Duration {
    type Error = InvalidQuantity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse().map(Self).map_err(|_| {
            if value.starts_with('-') {
                InvalidQuantity::Negative {
                    unit: "a duration",
                    value,
                }
            } else {
                InvalidQuantity::NotANumber {
                    unit: "seconds",
                    value,
                }
            }
        })
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.0)
    }
}

/// Ground covered, in millimetres.
///
/// Unsigned: you cannot cover a negative distance. Millimetres for the reason
/// grams serve a load — the value is persisted and compared, so it must not
/// depend on a float's rounding — and nothing outside this module sees them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Metres(u64);

impl Metres {
    pub const ZERO: Self = Self(0);

    pub const fn as_millimetres(self) -> u64 {
        self.0
    }

    pub const fn from_millimetres(millimetres: u64) -> Self {
        Self(millimetres)
    }
}

impl TryFrom<String> for Metres {
    type Error = InvalidQuantity;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let malformed = || InvalidQuantity::NotANumber {
            unit: "metres",
            value: value.clone(),
        };
        if value.starts_with('-') {
            return Err(InvalidQuantity::Negative {
                unit: "a distance",
                value,
            });
        }
        let (whole, fraction) = value.split_once('.').unwrap_or((value.as_str(), ""));
        if fraction.len() > 3 {
            return Err(malformed());
        }
        let mut padded = fraction.to_owned();
        while padded.len() < 3 {
            padded.push('0');
        }
        let whole: u64 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| malformed())?
        };
        let fraction: u64 = padded.parse().map_err(|_| malformed())?;
        whole
            .checked_mul(1_000)
            .and_then(|scaled| scaled.checked_add(fraction))
            .map(Self)
            .ok_or_else(malformed)
    }
}

impl fmt::Display for Metres {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / 1_000;
        let fraction = self.0 % 1_000;
        if fraction == 0 {
            write!(f, "{whole}m")
        } else {
            let fraction = format!("{fraction:03}");
            write!(f, "{whole}.{}m", fraction.trim_end_matches('0'))
        }
    }
}

/// Ground covered. A carry, a walking lunge, a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Distance {
    pub metres: Metres,
}

impl fmt::Display for Distance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.metres)
    }
}

crate::newtype::from_str_via_string!(RepCount, InvalidQuantity);
crate::newtype::from_str_via_string!(Duration, InvalidQuantity);
crate::newtype::from_str_via_string!(Metres, InvalidQuantity);
