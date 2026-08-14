//! What was on the bar, the stack, or the belt.
//!
//! Two things are settled here and neither is obvious from the types alone, so
//! the model of record carries the argument in full. In short:
//!
//! - **Load is a property of every set**, not a kind of set. A front squat and
//!   a box jump are both sets of reps; one is `Absolute(77.5)` and the other
//!   `Relative(0)`.
//! - **Absolute against relative is decided by whether zero is performable.**
//!   Where an unloaded version of the movement exists, zero is a real
//!   observation and the number is a delta against a bodyweight the set does
//!   not record. Where the implement has mass, zero is impossible — so a zero
//!   is a data error by construction, with no judgement required.
//!
//! There is deliberately no variant for a load that was not recorded. It was
//! tried and removed: it merged data that is wrong, a load that does not apply,
//! and a load that applies and was never captured, and deterministic
//! translation cannot tell them apart anyway.

use std::fmt;

/// Why a load could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidLoad {
    #[error("{value:?} is not a decimal number of kilograms")]
    NotDecimal { value: String },
    #[error("{value:?} carries more precision than a gram")]
    TooPrecise { value: String },
    #[error("a mass cannot be negative, and {value:?} is")]
    Negative { value: String },
}

/// How many decimal places survive. Three, so a gram is the smallest step.
const SCALE: i64 = 1_000;
const PLACES: usize = 3;

/// Parse a decimal string into thousandths, exactly.
///
/// Not via `f64`, which is the whole point: by the time you hold a float,
/// `20.4` is already `20.399999999999998578…` and every later conversion is
/// repair work. Loads are persisted, digested and compared against rows written
/// by earlier versions, so the value has to survive that round trip unchanged.
fn thousandths(value: &str) -> Result<i64, InvalidLoad> {
    let malformed = || InvalidLoad::NotDecimal {
        value: value.to_owned(),
    };

    let (sign, digits) = value
        .strip_prefix('-')
        .map_or((1_i64, value), |rest| (-1_i64, rest));
    let digits = digits.strip_prefix('+').unwrap_or(digits);

    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if whole.is_empty() && fraction.is_empty() {
        return Err(malformed());
    }
    if fraction.len() > PLACES {
        return Err(InvalidLoad::TooPrecise {
            value: value.to_owned(),
        });
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return Err(malformed());
    }

    let whole: i64 = if whole.is_empty() {
        0
    } else {
        whole.parse().map_err(|_| malformed())?
    };

    // Right-pad rather than parse-and-scale, so `.5` and `.500` reach the same
    // integer without a second rounding step.
    let mut padded = fraction.to_owned();
    while padded.len() < PLACES {
        padded.push('0');
    }
    let fraction: i64 = if padded.is_empty() {
        0
    } else {
        padded.parse().map_err(|_| malformed())?
    };

    whole
        .checked_mul(SCALE)
        .and_then(|scaled| scaled.checked_add(fraction))
        .and_then(|total| total.checked_mul(sign))
        .ok_or_else(malformed)
}

/// Render thousandths back as the shortest decimal that reads the same.
fn render(thousandths: i64) -> String {
    let sign = if thousandths < 0 { "-" } else { "" };
    let magnitude = thousandths.unsigned_abs();
    let whole = magnitude / (SCALE as u64);
    let fraction = magnitude % (SCALE as u64);
    if fraction == 0 {
        return format!("{sign}{whole}");
    }
    let fraction = format!("{fraction:03}");
    let fraction = fraction.trim_end_matches('0');
    format!("{sign}{whole}.{fraction}")
}

/// A mass. Never negative.
///
/// Holds grams, and no caller ever sees them: the only ways in and out are
/// `TryFrom<&str>` and `Display`, both of which speak kilograms. Fixed point
/// rather than a float because the value is persisted and compared (§ 7), and
/// the corpus holds `.1`, `.2` and `.4` hand-converted from pound-denominated
/// machines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Kg(i64);

impl Kg {
    pub const ZERO: Self = Self(0);

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub const fn as_grams(self) -> i64 {
        self.0
    }

    /// Rebuild from what was stored. `None` for a negative, which is a corrupt
    /// row rather than a rejected input — hence an `Option` and not the
    /// `InvalidLoad` that reports on text a source served.
    pub const fn from_grams(grams: i64) -> Option<Self> {
        if grams < 0 {
            return None;
        }
        Some(Self(grams))
    }
}

impl TryFrom<String> for Kg {
    type Error = InvalidLoad;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let grams = thousandths(&value)?;
        if grams < 0 {
            return Err(InvalidLoad::Negative { value });
        }
        Ok(Self(grams))
    }
}

impl fmt::Display for Kg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&render(self.0))
    }
}

/// A mass difference. Signed, because assistance and added weight are one axis.
///
/// The crossover through zero is a genuine progression — an assisted pull-up at
/// −20 becoming a weighted one at +10 — and it must not change type. Collapsing
/// "unassisted pull-up" and "pull-up with 0 kg assistance" into one series is
/// the motivating case for the whole load model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SignedKg(i64);

impl SignedKg {
    /// Plain bodyweight.
    pub const ZERO: Self = Self(0);

    pub const fn as_grams(self) -> i64 {
        self.0
    }

    pub const fn from_grams(grams: i64) -> Self {
        Self(grams)
    }

    /// Assistance, from a source that records it as a positive number.
    ///
    /// Hevy has no assistance concept — assisted movements are separately named
    /// exercises carrying a positive weight — so this is what the mapping
    /// applies to turn 20 into −20.
    #[must_use]
    pub const fn negated(self) -> Self {
        Self(self.0.saturating_neg())
    }
}

impl From<Kg> for SignedKg {
    fn from(mass: Kg) -> Self {
        Self(mass.as_grams())
    }
}

impl TryFrom<String> for SignedKg {
    type Error = InvalidLoad;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        thousandths(&value).map(Self)
    }
}

impl fmt::Display for SignedKg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&render(self.0))
    }
}

/// Why an absolute load could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("zero load on an exercise whose implement has mass")]
pub struct ZeroOnAbsoluteLoad;

/// What was being moved, beyond the body doing the moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Load {
    /// The implement has mass, so this is the whole load and zero is
    /// impossible.
    Absolute(Kg),
    /// A delta against a bodyweight the set does not record. Zero is plain
    /// bodyweight; negative is assistance.
    Relative(SignedKg),
}

impl Load {
    /// # Errors
    ///
    /// [`ZeroOnAbsoluteLoad`] for zero. The rule earns its place by
    /// self-checking: on an absolute-load exercise a zero is a data error with
    /// no judgement required, which is what makes the corpus's seven of them
    /// diagnosable rather than plausible.
    pub const fn absolute(mass: Kg) -> Result<Self, ZeroOnAbsoluteLoad> {
        if mass.is_zero() {
            return Err(ZeroOnAbsoluteLoad);
        }
        Ok(Self::Absolute(mass))
    }

    pub const fn relative(delta: SignedKg) -> Self {
        Self::Relative(delta)
    }

    /// Plain bodyweight, which is what most of the corpus's non-barbell work is.
    pub const BODYWEIGHT: Self = Self::Relative(SignedKg::ZERO);
}

impl fmt::Display for Load {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absolute(mass) => write!(f, "{mass} kg"),
            Self::Relative(delta) if delta.as_grams() == 0 => f.write_str("bodyweight"),
            Self::Relative(delta) if delta.as_grams() < 0 => write!(f, "bodyweight {delta} kg"),
            Self::Relative(delta) => write!(f, "bodyweight +{delta} kg"),
        }
    }
}

crate::newtype::from_str_via_string!(Kg, InvalidLoad);
crate::newtype::from_str_via_string!(SignedKg, InvalidLoad);
