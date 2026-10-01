//! A value, and the account it was taken from.

use std::fmt;

use super::session::NormalisedSessionId;

/// One value of a canonical entity, and the normalised session it came from.
///
/// **The unit of attribution is the field, not the entity and not the set.**
/// § 10, amended 2026-10-01: a canonical entity is assembled field by field
/// from whichever accounts recorded each field. On 2020-10-09 one canonical
/// set holds its reps from `the_beginner_prescription.xlsx` and its load from
/// the watch, because the sheet's load column was a formula that came back
/// empty and the operator put the real load on Garmin. A set that named one
/// account could not say that.
///
/// **There is no second value here.** Where two accounts recorded one field
/// and differ because one of them could not express what the other gave — 48
/// kg on a dial that takes whole kilogrammes against 47.5 kg written down —
/// this holds the one that could, and the other stands unchanged in the
/// normalised layer. Holding both would be a canonical entity that names its
/// accounts instead of merging them, which is what #297 did and what the
/// operator rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attributed<T> {
    value: T,
    account: NormalisedSessionId,
}

impl<T> Attributed<T> {
    pub const fn new(value: T, account: NormalisedSessionId) -> Self {
        Self { value, account }
    }

    pub const fn value(&self) -> &T {
        &self.value
    }

    /// The normalised session this value was taken from.
    pub const fn account(&self) -> NormalisedSessionId {
        self.account
    }

    /// The value, given up, where a caller is past caring where it came from.
    pub fn into_value(self) -> T {
        self.value
    }

    /// The same attribution over a value derived from this one.
    ///
    /// The account travels with it, because a value computed from one
    /// account's value is still that account's claim and nothing else's.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Attributed<U> {
        Attributed {
            value: f(self.value),
            account: self.account,
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
        write!(f, "{} ({})", self.value, self.account)
    }
}
