//! A mass: on a bar, or on a scale.
//!
//! **Here rather than in `gym`**, because a weigh-in is counted in it too, and
//! a body mass is not a load. The type was the gym's while the gym was the only
//! thing that weighed anything.

use std::fmt;

/// Why a mass could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidMass {
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
pub fn thousandths(value: &str) -> Result<i64, InvalidMass> {
    let malformed = || InvalidMass::NotDecimal {
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
        return Err(InvalidMass::TooPrecise {
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
pub fn render(grams: i64) -> String {
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
    /// [`InvalidMass`] if the value is not a decimal number of pounds.
    pub fn from_pounds(value: &str) -> Result<Self, InvalidMass> {
        let thousandths_of_a_pound = thousandths(value)?;
        if thousandths_of_a_pound < 0 {
            return Err(InvalidMass::Negative {
                value: value.to_owned(),
            });
        }
        // Thousandths of a pound times grams per pound, divided back down by
        // the thousandth. Integer throughout, so nothing rounds twice.
        let grams = thousandths_of_a_pound
            .checked_mul(GRAMS_PER_POUND)
            .map(|scaled| scaled / SCALE)
            .ok_or_else(|| InvalidMass::NotDecimal {
                value: value.to_owned(),
            })?;
        u64::try_from(grams)
            .map(Self)
            .map_err(|_| InvalidMass::Negative {
                value: value.to_owned(),
            })
    }
}

impl TryFrom<String> for Kg {
    type Error = InvalidMass;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let grams = thousandths(&value)?;
        u64::try_from(grams)
            .map(Self)
            .map_err(|_| InvalidMass::Negative { value })
    }
}

impl fmt::Display for Kg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let grams = i64::try_from(self.0).unwrap_or(i64::MAX);
        f.write_str(&render(grams))
    }
}

crate::newtype::from_str_via_string!(Kg, InvalidMass);
