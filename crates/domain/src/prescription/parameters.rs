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

use std::{collections::BTreeMap, fmt};

use crate::gym::{
    Duration, Kg, RepCount,
    exercise::{Exercise, Implement},
};

use super::{steps::LoadSteps, target::Target};

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

/// Every implement's load scale, as one authored set.
///
/// A fact about this gym's equipment (§ 14) rather than about the programme,
/// which is why it is data where the rounding rule that consumes it is code
/// (§ 9).
///
/// **An implement with no scale makes the exercise underivable, never
/// defaulted.** Borrowing the barbell's steps for a sled would produce a
/// prescription indistinguishable from one derived from a real rack, and a
/// placeholder that authors successfully is worse than one that fails.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Scales(BTreeMap<Implement, LoadSteps>);

impl Scales {
    pub const fn new(scales: BTreeMap<Implement, LoadSteps>) -> Self {
        Self(scales)
    }

    /// The scale in force for an implement, if one has been authored.
    #[must_use]
    pub fn for_implement(&self, implement: Implement) -> Option<&LoadSteps> {
        self.0.get(&implement)
    }

    /// The scale an exercise is loaded on.
    #[must_use]
    pub fn for_exercise(&self, exercise: Exercise) -> Option<&LoadSteps> {
        self.for_implement(exercise.implement())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Implement, &LoadSteps)> {
        self.0.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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

/// The primary's back-off sets, for one session role.
///
/// **Its own numbers, per role.** These used to be read off the strength
/// block's [`AccessoryScheme`] on the grounds that the primary is a strength
/// slot and nobody had stated otherwise — which issued the light session's
/// three sets of six on the heavy day. The operator stated it on 2026-08-20:
/// heavy is `1 @ x, 2 × 4`, light is `3 @ x, 3 × 6`, and the record agrees on
/// every session since the July test.
///
/// The percentage lives here rather than beside it because that is how the two
/// patterns were stated — as patterns, each complete. Both are 85% today and
/// nothing requires them to stay equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackOff {
    pub sets: RepCount,
    pub reps: RepCount,
    /// Of this session's own top set, never of the anchor.
    pub of_top_set: Percentage,
}

/// The double-progression scheme one block's slots run.
///
/// Work the range, and when the top of it is reached at every working set, add an
/// increment and start again at the bottom.
///
/// **One scheme per block, not one per slot and not one for everything.** The
/// slots within a block are prescribed alike — every non-primary strength slot
/// shares a scheme, and so does every hypertrophy slot — while the two blocks
/// differ from each other. A per-slot scheme is a larger authored surface that
/// nothing yet needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessoryScheme {
    /// The rep target, as a target. **Not two loose bounds**: a `low` and a
    /// `high` beside each other can be written down inverted, and the check that
    /// would catch it belongs in the type rather than at every call site
    /// (§ 24). See [`Target`].
    pub reps: Target<RepCount>,
    pub sets: RepCount,
}

/// Everything consulted when generating, in force as one version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationParameters {
    pub warmup: crate::gym::NonEmpty<WarmupStep>,
    /// The primary's back-off sets. Per role, because the two roles differ.
    pub back_off: super::schedule::PerRole<BackOff>,
    /// The light session's top set, as a proportion of that week's heavy one.
    /// Deriving it from the heavy load rather than from the anchor is what makes
    /// the two roles move together by construction.
    pub light_of_heavy: Percentage,
    /// What the plan adds each climbing week. There is no authored endpoint:
    /// the climb runs until the calendar stops it, and what regulates it is the
    /// reset protocol rather than a stated top. Same kind as
    /// [`ResetProtocol::reclimb_per_week`], because a reset is this climb run at
    /// a different rate off a lower start.
    ///
    /// **There is no opening percentage beside it.** Where the ladder opens is
    /// derived from the entry test the anchor records, not authored — see
    /// `docs/decisions/0009-a-linear-block-opens-from-its-entry-test.md`.
    pub ladder_climb_per_week: Kg,
    /// Negative. What a block's opening drops off the load its entry test
    /// failed, where the opening is derived rather than declared.
    ///
    /// **Authored, not borrowed from a reset.** It lands on the same −10% the
    /// first reset drops by, and it is stated separately anyway: the two agree
    /// today by decision rather than by derivation, and a composed default that
    /// nothing pins is exactly the class of fault that produced `/v1/v1`.
    pub entry_drop: Percentage,
    pub top_set_reps: super::schedule::PerRole<TopSetReps>,
    /// Every non-primary strength slot.
    pub strength: AccessoryScheme,
    /// Every hypertrophy slot.
    pub hypertrophy: AccessoryScheme,
    /// How long a static hold is held for.
    ///
    /// The mobility work does not progress — it is held, and the same length
    /// every time — so its prescription comes from here rather than from
    /// history. One duration for every static slot.
    pub static_hold: Duration,
    /// What each implement can hold. Consulted wherever a derived load has to
    /// land on something the gym owns.
    pub scales: Scales,
    pub first_reset: ResetProtocol,
    pub second_reset: ResetProtocol,
}

crate::newtype::from_str_via_string!(Percentage, InvalidPercentage);
