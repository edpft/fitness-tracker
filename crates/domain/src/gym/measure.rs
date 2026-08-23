//! What a set is counted in.
//!
//! Three measures, because there are three things you can count: repetitions,
//! elapsed time, and ground covered.
//!
//! An exercise's measure is fixed by which vocabulary it belongs to, so a set
//! and its exercise cannot disagree and nothing needs validating.

use std::{
    fmt,
    num::{NonZeroU32, NonZeroU64},
};

/// Why a quantity could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidQuantity {
    #[error("{value:?} is not a whole number of {unit}")]
    NotANumber { unit: &'static str, value: String },
    #[error("{unit} cannot be negative, and {value:?} is")]
    Negative { unit: &'static str, value: String },
    #[error("a set of zero reps is not a set")]
    ZeroReps,
    #[error("a range spans a positive amount, and zero is not one")]
    ZeroExtent,
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

/// A strictly positive length of time.
///
/// **Not a [`Duration`], and the difference is the point.** A duration may be
/// zero — a superset instructs exactly that, "go straight on" — so a duration
/// cannot stand for the *width* of a range without letting a range of no width
/// be written down. This can only be positive, which is what makes an empty
/// range unrepresentable rather than merely rejected (§ 24).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PositiveDuration(NonZeroU64);

impl PositiveDuration {
    /// # Errors
    ///
    /// [`InvalidQuantity::ZeroExtent`] for a zero, which spans nothing.
    pub const fn from_seconds(seconds: u64) -> Result<Self, InvalidQuantity> {
        match NonZeroU64::new(seconds) {
            Some(seconds) => Ok(Self(seconds)),
            None => Err(InvalidQuantity::ZeroExtent),
        }
    }

    pub const fn as_seconds(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for PositiveDuration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}s", self.0)
    }
}

/// A strictly positive amount of ground. [`PositiveDuration`]'s reasoning, for
/// distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PositiveDistance(NonZeroU64);

impl PositiveDistance {
    /// # Errors
    ///
    /// [`InvalidQuantity::ZeroExtent`] for a zero, which spans nothing.
    pub const fn from_millimetres(millimetres: u64) -> Result<Self, InvalidQuantity> {
        match NonZeroU64::new(millimetres) {
            Some(millimetres) => Ok(Self(millimetres)),
            None => Err(InvalidQuantity::ZeroExtent),
        }
    }

    pub const fn as_millimetres(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Display for PositiveDistance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Metres(self.0.get()))
    }
}

/// A measure a range can be built over.
///
/// **A range is a minimum and an extent, never two endpoints.** Two endpoints
/// are two independent values, and nothing in the type stops the second being
/// below the first — the best a pair can do is reject the inversion at
/// construction, which § 24 says is the wrong place. A minimum and a strictly
/// positive extent cannot describe an empty or inverted range at all, so the
/// check disappears rather than moving.
///
/// The extent is an associated type rather than `Self` because only some of
/// these measures exclude zero. [`RepCount`] already does, so it is its own
/// extent; a duration and a distance do not, so they have positive counterparts.
pub trait Spans: Copy {
    /// A strictly positive amount of the same quantity.
    type Extent: Copy + fmt::Debug + fmt::Display + PartialEq + Eq;

    /// The top of a range that opens at `self` and spans `extent`.
    ///
    /// Saturating, because a range whose top overflows is a range that reached
    /// the largest value the measure has — which is the answer, not a failure.
    #[must_use]
    fn spanning(self, extent: Self::Extent) -> Self;

    /// The extent between two bounds, where they are a range at all.
    ///
    /// The one fallible direction, and it exists for reading back a pair of
    /// bounds that something outside the domain wrote down.
    fn extent_between(low: Self, high: Self) -> Option<Self::Extent>;
}

impl Spans for RepCount {
    /// Its own extent: a rep count is already non-zero, so it cannot span
    /// nothing.
    type Extent = Self;

    fn spanning(self, extent: Self) -> Self {
        Self(self.0.saturating_add(extent.0.get()))
    }

    fn extent_between(low: Self, high: Self) -> Option<Self> {
        NonZeroU32::new(high.0.get().checked_sub(low.0.get())?).map(Self)
    }
}

impl Spans for Duration {
    type Extent = PositiveDuration;

    fn spanning(self, extent: PositiveDuration) -> Self {
        Self(self.0.saturating_add(extent.as_seconds()))
    }

    fn extent_between(low: Self, high: Self) -> Option<PositiveDuration> {
        PositiveDuration::from_seconds(high.0.checked_sub(low.0)?).ok()
    }
}

impl Spans for Distance {
    type Extent = PositiveDistance;

    fn spanning(self, extent: PositiveDistance) -> Self {
        Self {
            metres: Metres(self.metres.0.saturating_add(extent.as_millimetres())),
        }
    }

    fn extent_between(low: Self, high: Self) -> Option<PositiveDistance> {
        PositiveDistance::from_millimetres(high.metres.0.checked_sub(low.metres.0)?).ok()
    }
}
