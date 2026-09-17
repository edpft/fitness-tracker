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

use crate::measure::{
    InvalidMass, Kg,
    mass::{render, thousandths},
};

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
    type Error = InvalidMass;

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

/// Heavier than, within one axis.
///
/// **Partial, and hand-written so it cannot be derived by accident.** Deriving
/// would order by variant and declare every `Absolute` lighter than every
/// `Relative`, which is not a fact about anything. Two axes do not compare: a
/// bodyweight squat is `Absolute(0)` and a plain bodyweight pull-up is
/// `Relative(0)`, they are not the same load, and neither is heavier.
///
/// Within an axis both are the obvious thing, and the relative one carries the
/// crossover the axis exists for: −20 is lighter than −5, which is lighter than
/// bodyweight, which is lighter than +10. Less assistance is heavier.
///
/// There is no `Ord`, so `Iterator::max` will not reach for this. That is
/// deliberate: a caller taking the heaviest of a collection has to say what it
/// means to find two it cannot order.
impl PartialOrd for Load {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Self::Absolute(ours), Self::Absolute(theirs)) => Some(ours.cmp(theirs)),
            (Self::Relative(ours), Self::Relative(theirs)) => Some(ours.cmp(theirs)),
            (Self::Absolute(_), Self::Relative(_)) | (Self::Relative(_), Self::Absolute(_)) => None,
        }
    }
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

crate::newtype::from_str_via_string!(SignedKg, InvalidMass);
