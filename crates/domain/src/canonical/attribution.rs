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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attributed<T> {
    value: T,
    normalised_session: NormalisedSessionId,
}

impl<T> Attributed<T> {
    pub const fn new(value: T, normalised_session: NormalisedSessionId) -> Self {
        Self {
            value,
            normalised_session,
        }
    }

    pub const fn value(&self) -> &T {
        &self.value
    }

    /// The normalised session this value was taken from.
    pub const fn normalised_session(&self) -> NormalisedSessionId {
        self.normalised_session
    }

    /// The value, given up, where a caller is past caring where it came from.
    pub fn into_value(self) -> T {
        self.value
    }

    /// The same attribution over a value derived from this one.
    ///
    /// The attribution travels with it, because a value computed from one
    /// normalised session's value is still that session's claim and nothing
    /// else's.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Attributed<U> {
        Attributed {
            value: f(self.value),
            normalised_session: self.normalised_session,
        }
    }
}

impl<T: Copy> Attributed<T> {
    /// The value where it is cheap to copy, which most of them are.
    pub const fn copied(&self) -> T {
        self.value
    }
}

impl<T: fmt::Display> fmt::Display for Attributed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.value, self.normalised_session)
    }
}
