//! A value, and the normalised session it was taken from.

use std::fmt;

use super::session::NormalisedSessionId;

/// One value of a canonical entity, and the normalised session it came from.
///
/// **The unit of attribution is the field, not the entity and not the set.**
/// § 10, amended 2026-10-01: a canonical entity is assembled field by field
/// from whichever normalised sessions recorded each field. On 2020-10-09 one
/// canonical set holds its reps from `the_beginner_prescription.xlsx` and its
/// load from the watch, because the sheet's load column was a formula that came
/// back empty and the operator put the real load on Garmin. A set that named
/// one normalised session could not say that.
///
/// **A normalised session, not a source.** A source is a system — Hevy, Garmin,
/// the spreadsheets — and two of 2019-03-14's three accounts of that visit are
/// the same source: `1RM.xlsx` and `Strength training 2019.xlsx` both landed
/// under `spreadsheets.files`, and telling them apart is the whole point of that
/// day. Naming the source would say nothing about which workbook.
///
/// **There is no second value here.** Where two normalised sessions recorded one
/// field and differ because one of them could not express what the other gave —
/// 48 kg on a dial that takes whole kilogrammes against 47.5 kg written down —
/// this holds the one that could, and the other stands unchanged in the
/// normalised layer. Holding both would be a canonical entity that names what it
/// stands on instead of merging it, which is what #297 did and what the operator
/// rejected.
///
/// **Generic over what it names**, because not every normalised entity is a
/// session: a weigh-in's mass names the normalised weigh-in it was taken from
/// (#409).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attributed<T, By = NormalisedSessionId> {
    value: T,
    taken_from: By,
}

impl<T> Attributed<T> {
    /// The normalised session this value was taken from.
    pub const fn normalised_session(&self) -> NormalisedSessionId {
        self.taken_from
    }
}

impl<T> Attributed<T, TakenFrom> {
    /// The normalised session this value was taken from, or [`None`] where
    /// the operator's edit stated it.
    pub const fn normalised(&self) -> Option<NormalisedSessionId> {
        match self.taken_from {
            TakenFrom::Normalised(session) => Some(session),
            TakenFrom::Edit => None,
        }
    }

    /// Whether the operator's edit stated it.
    pub const fn is_edited(&self) -> bool {
        matches!(self.taken_from, TakenFrom::Edit)
    }

    /// A value the operator's edit states (§ II.2).
    pub const fn edited(value: T) -> Self {
        Self {
            value,
            taken_from: TakenFrom::Edit,
        }
    }
}

impl<T, By: Copy> Attributed<T, By> {
    /// `taken_from` is anything that names what `By` does: a field of a
    /// canonical gym set is given the normalised session it was read from.
    pub fn new(value: T, taken_from: impl Into<By>) -> Self {
        Self {
            value,
            taken_from: taken_from.into(),
        }
    }

    pub const fn value(&self) -> &T {
        &self.value
    }

    /// The normalised entity this value was taken from.
    pub const fn taken_from(&self) -> By {
        self.taken_from
    }

    /// The same attribution over a value derived from this one.
    ///
    /// The attribution travels with it, because a value computed from one
    /// normalised session's value is still that session's claim and nothing
    /// else's.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Attributed<U, By> {
        Attributed {
            value: f(self.value),
            taken_from: self.taken_from,
        }
    }
}

impl<T: Copy, By: Copy> Attributed<T, By> {
    /// The value where it is cheap to copy, which most of them are.
    pub const fn copied(&self) -> T {
        self.value
    }
}

impl<T: fmt::Display, By: fmt::Display> fmt::Display for Attributed<T, By> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.value, self.taken_from)
    }
}

/// Where a field of a canonical entity the operator can edit was taken from:
/// a normalised session, or his edit (§ II.2, #893).
///
/// **The edit names no normalised session**, because it is not one: a reps in
/// reserve he gave a watch's set was never recorded anywhere. What the edit
/// was, when and why, is read from the overlay by the entity's own id.
///
/// **Ordered with every normalised session before the edit**, so a merge that
/// takes the lowest gives the same answer it did; an edit never reaches one,
/// since it is applied after matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TakenFrom {
    Normalised(NormalisedSessionId),
    Edit,
}

impl From<NormalisedSessionId> for TakenFrom {
    fn from(session: NormalisedSessionId) -> Self {
        Self::Normalised(session)
    }
}

impl fmt::Display for TakenFrom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normalised(session) => write!(f, "{session}"),
            Self::Edit => f.write_str("an edit"),
        }
    }
}
