//! The values consulted when authoring a prescription (§ 14).
//!
//! Only the current value is required, and that is not laziness: what a
//! parameter produced is recorded concretely on the prescription that used it,
//! so a superseded percentage answers no question. History is kept anyway
//! because it costs nothing, and no derivation reads it.
//!
//! **Percentages are integers.** Basis points, for the same reason [`Kg`] holds
//! grams: the value is persisted and a stored prescription that cannot be
//! reproduced byte for byte is not a record of anything. A float would make
//! `85%` into `0.8500000000000000888…` and every later comparison repair work.
//!
//! [`Kg`]: crate::gym::Kg

use std::fmt;

use crate::gym::{Kg, RepCount};

/// Why a percentage could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidPercentage {
    #[error("{value:?} is not a percentage")]
    NotAPercentage { value: String },
    #[error("{value:?} carries more precision than a hundredth of a percent")]
    TooPrecise { value: String },
    #[error("a percentage of zero prescribes nothing, and {value:?} is one")]
    Zero { value: String },
}

/// Hundredths of a percent. Two decimal places, so `2.08%` survives.
const BASIS_POINTS_PER_PERCENT: i32 = 100;
/// The whole, in basis points. What a factor of 1 is.
const WHOLE_BASIS_POINTS: i32 = 100 * BASIS_POINTS_PER_PERCENT;
const PLACES: usize = 2;

/// A proportion, held as basis points.
///
/// Signed, because a reset drop is a negative proportion and putting it on the
/// same axis as every other percentage is what lets one quantisation rule serve
/// all of them.
///
/// Zero is rejected. A parameter set to zero would prescribe an empty bar or a
/// step that never moves, and both are configuration mistakes rather than
/// intentions anyone holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Percentage(i32);

impl Percentage {
    /// The whole, exactly. What a ladder passes through on its way past a
    /// tested maximum.
    pub const WHOLE: Self = Self(WHOLE_BASIS_POINTS);

    pub const fn as_basis_points(self) -> i32 {
        self.0
    }

    /// Rebuild from stored basis points.
    ///
    /// # Errors
    ///
    /// [`InvalidPercentage::Zero`] for zero, which the store's own `CHECK`
    /// constraints also refuse.
    pub const fn from_basis_points(points: i32) -> Result<Self, InvalidPercentage> {
        if points == 0 {
            return Err(InvalidPercentage::Zero {
                value: String::new(),
            });
        }
        Ok(Self(points))
    }

    /// This proportion of a mass, quantised nowhere.
    ///
    /// Integer throughout and rounded once, at the end, toward zero. Callers
    /// wanting the plate grid pass the result through
    /// [`quantise`](super::quantise::quantise) — which is deliberately a
    /// separate step, because the grid is a fact about equipment and this is
    /// arithmetic.
    #[must_use]
    pub fn of(self, mass: Kg) -> Kg {
        Self::scale(mass, self.0)
    }

    /// This proportion added to the whole — what a −10% drop means applied to a
    /// load.
    ///
    /// `Percentage(-1000).applied_to(90kg)` is 81kg, where
    /// `Percentage(-1000).of(90kg)` is 0: one is a change and the other is a
    /// share, and conflating them is how a reset lands on the wrong bar.
    #[must_use]
    pub fn applied_to(self, mass: Kg) -> Kg {
        Self::scale(mass, WHOLE_BASIS_POINTS.saturating_add(self.0))
    }

    /// A mass times a basis-point factor, in integers and without a cast that
    /// could truncate.
    ///
    /// A negative or zero factor is no load: a drop larger than the whole
    /// removes more than there was, and there is no such bar.
    fn scale(mass: Kg, points: i32) -> Kg {
        let Ok(points) = u64::try_from(points) else {
            return Kg::NONE;
        };
        let Ok(whole) = u64::try_from(WHOLE_BASIS_POINTS) else {
            return Kg::NONE;
        };
        mass.as_grams()
            .checked_mul(points)
            .map_or(Kg::NONE, |scaled| Kg::from_grams(scaled / whole))
    }
}

/// Parse `85%`, `-10%`, `2.08%`. The suffix is required: a bare number in an
/// authored document is ambiguous between a percentage and a proportion, and
/// the document exists to be read by a person.
impl TryFrom<String> for Percentage {
    type Error = InvalidPercentage;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let malformed = || InvalidPercentage::NotAPercentage {
            value: value.clone(),
        };

        let body = value.strip_suffix('%').ok_or_else(malformed)?;
        let (sign, digits) = body
            .strip_prefix('-')
            .map_or((1_i32, body), |rest| (-1_i32, rest));
        let digits = digits.strip_prefix('+').unwrap_or(digits);

        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        if whole.is_empty() && fraction.is_empty() {
            return Err(malformed());
        }
        if fraction.len() > PLACES {
            return Err(InvalidPercentage::TooPrecise { value });
        }
        if !whole.chars().all(|c| c.is_ascii_digit())
            || !fraction.chars().all(|c| c.is_ascii_digit())
        {
            return Err(malformed());
        }

        let whole: i32 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| malformed())?
        };
        let mut padded = fraction.to_owned();
        while padded.len() < PLACES {
            padded.push('0');
        }
        let fraction: i32 = padded.parse().map_err(|_| malformed())?;

        let points = whole
            .checked_mul(BASIS_POINTS_PER_PERCENT)
            .and_then(|scaled| scaled.checked_add(fraction))
            .and_then(|total| total.checked_mul(sign))
            .ok_or_else(malformed)?;

        if points == 0 {
            return Err(InvalidPercentage::Zero { value });
        }
        Ok(Self(points))
    }
}

impl fmt::Display for Percentage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let magnitude = self.0.unsigned_abs();
        let whole = magnitude / BASIS_POINTS_PER_PERCENT.unsigned_abs();
        let fraction = magnitude % BASIS_POINTS_PER_PERCENT.unsigned_abs();
        if fraction == 0 {
            return write!(f, "{sign}{whole}%");
        }
        let rendered = format!("{fraction:02}");
        write!(f, "{sign}{whole}.{}%", rendered.trim_end_matches('0'))
    }
}

/// The smallest load step the equipment allows.
///
/// A fact about the gym rather than about the programme, which is why it is
/// data where the rounding rule that consumes it is code (§ 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlateIncrement(Kg);

impl PlateIncrement {
    /// # Errors
    ///
    /// [`InvalidIncrement`] for zero, which would make every load land on
    /// itself and the quantiser divide by nothing.
    pub const fn new(step: Kg) -> Result<Self, InvalidIncrement> {
        if step.as_grams() == 0 {
            return Err(InvalidIncrement);
        }
        Ok(Self(step))
    }

    pub const fn as_kg(self) -> Kg {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a plate increment of zero is not a step")]
pub struct InvalidIncrement;

impl fmt::Display for PlateIncrement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How many repetitions a top set is prescribed for.
///
/// A count and not a range: the primary's top set is executed as written, which
/// is what makes it pass or fail rather than scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopSetReps(RepCount);

impl TopSetReps {
    pub const fn new(reps: RepCount) -> Self {
        Self(reps)
    }

    pub const fn as_rep_count(self) -> RepCount {
        self.0
    }
}

impl fmt::Display for TopSetReps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One step of the ramp before a top set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WarmupStep {
    /// Of the session's own top set, never of the anchor.
    pub of_top_set: Percentage,
    pub reps: RepCount,
}

/// What a stall costs, and how the ground is re-covered.
///
/// The drop and the increment are chosen as a pair so both land on the plate
/// grid and both cost the same four weeks — so a stall has a fixed price
/// whichever reset is in play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResetProtocol {
    /// Negative. Taken from the failed load, never from the anchor.
    pub drop: Percentage,
    pub reclimb_per_week: Kg,
}

/// The scheme every non-primary strength and hypertrophy slot runs.
///
/// Double progression: work the range, and when the top of it is reached at every
/// working set, add an increment and start again at the bottom.
///
/// **One range for all of them, which is a simplification.** The record runs
/// pull-ups at six, curls around four to six and wrist work at six — close enough
/// that one range reproduces the shape, and different enough that a per-slot range
/// would be more faithful. Deferred rather than hidden: a slot-keyed range is a
/// bigger authored surface and nothing yet needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessoryScheme {
    pub low: RepCount,
    pub high: RepCount,
    pub sets: RepCount,
}

/// Everything consulted when generating, in force as one version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationParameters {
    pub warmup: crate::gym::NonEmpty<WarmupStep>,
    pub back_off_of_top_set: Percentage,
    /// The light session's top set, as a proportion of that week's heavy one.
    /// Deriving it from the heavy load rather than from the anchor is what makes
    /// the two roles move together by construction.
    pub light_of_heavy: Percentage,
    pub ladder_start: Percentage,
    pub ladder_end: Percentage,
    pub top_set_reps: super::schedule::PerRole<TopSetReps>,
    pub accessory: AccessoryScheme,
    pub plate_increment: PlateIncrement,
    pub first_reset: ResetProtocol,
    pub second_reset: ResetProtocol,
}

crate::newtype::from_str_via_string!(Percentage, InvalidPercentage);
