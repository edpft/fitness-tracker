//! What was being moved, beyond the body doing the moving.
//!
//! **Load is a property of every set, not a kind of set.** A front squat and a
//! box jump are both sets of reps; what differs is the load each carries.
//!
//! **Absolute or relative is a question about the exercise, not about the
//! number.** It asks whether the load axis runs in both directions: whether
//! assistance is conventionally available as well as added weight.
//!
//! A pull-up is `Relative`. The bodyweight version is the movement, machines
//! and bands routinely make it easier, and a belt or a dumbbell routinely makes
//! it harder — so the axis passes through zero and the sign carries meaning. A
//! squat is `Absolute`. Adding weight is the whole progression and taking
//! weight away is not a thing anyone does, so the number is simply how much was
//! on the bar, and none is a real answer.
//!
//! This is a convention rather than a physical fact, which is why it is decided
//! per exercise in the mapping and not inferred from any value. An exercise
//! that becomes conventionally assisted moves; nothing about the data moves it.

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

/// Grams in a pound, exactly. The international avoirdupois pound is defined in
/// terms of the kilogram, so the conversion is exact rather than approximate.
const GRAMS_PER_POUND: i64 = 453_592;

/// Parse a decimal string into thousandths, exactly.
///
/// Not via `f64`, which is the point: by the time you hold a float, `20.4` is
/// already `20.399999999999998578…` and every later conversion is repair work.
/// Loads are persisted and compared against rows written by earlier versions, so
/// the value has to survive that round trip unchanged.
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
    let fraction: i64 = padded.parse().map_err(|_| malformed())?;

    whole
        .checked_mul(SCALE)
        .and_then(|scaled| scaled.checked_add(fraction))
        .and_then(|total| total.checked_mul(sign))
        .ok_or_else(malformed)
}

/// Render grams back as the shortest decimal that reads the same.
fn render(grams: i64) -> String {
    let sign = if grams < 0 { "-" } else { "" };
    let magnitude = grams.unsigned_abs();
    let whole = magnitude / 1_000;
    let fraction = magnitude % 1_000;
    if fraction == 0 {
        return format!("{sign}{whole}");
    }
    let fraction = format!("{fraction:03}");
    let fraction = fraction.trim_end_matches('0');
    format!("{sign}{whole}.{fraction}")
}

/// A mass.
///
/// Unsigned, because there is no such thing as a negative amount of weight on a
/// bar. A load that is a *difference* — assistance against added weight — is
/// [`SignedKg`], and keeping the two apart means no caller has to remember
/// which of them it is holding.
///
/// Holds grams, and no caller sees them: the ways in and out are
/// `TryFrom<&str>`, [`Self::from_pounds`] and `Display`, all of which speak
/// whole units. Fixed point rather than a float because the value is persisted
/// and compared, and the corpus holds `.1`, `.2` and `.4`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Kg(u64);

impl Kg {
    /// No external load. A real observation, not an absence: a bodyweight
    /// squat, a set of running, an unloaded stretch.
    pub const NONE: Self = Self(0);

    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    pub const fn as_grams(self) -> u64 {
        self.0
    }

    pub const fn from_grams(grams: u64) -> Self {
        Self(grams)
    }

    /// A mass read off a machine labelled in pounds.
    ///
    /// Exact, because the pound is defined in terms of the kilogram. Converting
    /// by hand before entry is what has been happening, and it loses precision
    /// in a way this does not — so a source that serves pounds converts here
    /// rather than at a keyboard.
    ///
    /// # Errors
    ///
    /// [`InvalidLoad`] if the value is not a decimal number of pounds.
    pub fn from_pounds(value: &str) -> Result<Self, InvalidLoad> {
        let thousandths_of_a_pound = thousandths(value)?;
        if thousandths_of_a_pound < 0 {
            return Err(InvalidLoad::Negative {
                value: value.to_owned(),
            });
        }
        // Thousandths of a pound times grams per pound, divided back down by
        // the thousandth. Integer throughout, so nothing rounds twice.
        let grams = thousandths_of_a_pound
            .checked_mul(GRAMS_PER_POUND)
            .map(|scaled| scaled / SCALE)
            .ok_or_else(|| InvalidLoad::NotDecimal {
                value: value.to_owned(),
            })?;
        u64::try_from(grams)
            .map(Self)
            .map_err(|_| InvalidLoad::Negative {
                value: value.to_owned(),
            })
    }
}

impl TryFrom<String> for Kg {
    type Error = InvalidLoad;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let grams = thousandths(&value)?;
        u64::try_from(grams)
            .map(Self)
            .map_err(|_| InvalidLoad::Negative { value })
    }
}

impl fmt::Display for Kg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let grams = i64::try_from(self.0).unwrap_or(i64::MAX);
        f.write_str(&render(grams))
    }
}

/// A mass difference. Signed, because assistance and added weight are one axis.
///
/// The crossover through zero is a genuine progression — an assisted pull-up at
/// −20 becoming a weighted one at +10 — and it must not change type. Collapsing
/// "unassisted pull-up" and "pull-up with 0 kg assistance" into one series is
/// the motivating case for the whole load model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
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
        Self(i64::try_from(mass.as_grams()).unwrap_or(i64::MAX))
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

/// What was being moved.
///
/// Two variants and no third. There is deliberately no case for a load that was
/// not recorded: it would merge data that is wrong, a load that does not apply,
/// and a load that applies and was never captured — and deterministic
/// translation cannot tell those apart from the value alone, which is why the
/// distinction belongs to the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Load {
    /// External load, measured from nothing. Zero is no external load, which is
    /// a real observation — a bodyweight squat, a set of skipping.
    Absolute(Kg),
    /// A delta against a bodyweight the set does not record, on an axis where
    /// assistance is conventionally available. Zero is plain bodyweight;
    /// negative is assistance.
    Relative(SignedKg),
}

impl Load {
    pub const fn absolute(mass: Kg) -> Self {
        Self::Absolute(mass)
    }

    pub const fn relative(delta: SignedKg) -> Self {
        Self::Relative(delta)
    }

    /// No external load, on an absolute exercise.
    pub const UNLOADED: Self = Self::Absolute(Kg::NONE);

    /// Plain bodyweight, on a relative one — an unassisted pull-up.
    pub const BODYWEIGHT: Self = Self::Relative(SignedKg::ZERO);
}

impl fmt::Display for Load {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Absolute(mass) if mass.is_none() => f.write_str("no external load"),
            Self::Absolute(mass) => write!(f, "{mass} kg"),
            Self::Relative(delta) if delta.as_grams() == 0 => f.write_str("bodyweight"),
            Self::Relative(delta) if delta.as_grams() < 0 => write!(f, "bodyweight {delta} kg"),
            Self::Relative(delta) => write!(f, "bodyweight +{delta} kg"),
        }
    }
}

crate::newtype::from_str_via_string!(Kg, InvalidLoad);
crate::newtype::from_str_via_string!(SignedKg, InvalidLoad);
