//! How a canonical entity names a normalised one.

use std::fmt;

/// The normalised session a canonical entity stands on.
///
/// **An id the store assigns, for the reason
/// [`crate::landing::LandingRecordId`] is one.** The alternative is the
/// record it landed as, which is what every normalised gym session already
/// carries — and it does not name a session. One Beyond The White Board export
/// holds 36 of the operator's sessions and `the_beginner_prescription.xlsx`
/// holds a hundred, all under one landing record, so the landing record names
/// the file. Adding the day to it names a session only until two are trained
/// on one day, which the record already has.
///
/// **It is not an overlay anchor, which is the case § II.2 forbids a surrogate
/// for.** An overlay must survive a rebuild that reassigns every derived id;
/// a canonical entity is itself rebuilt by that same rebuild, from the
/// normalised rows as they then are. When the match overlay arrives with #247
/// it anchors to source identity as § II.2 requires, and not to this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NormalisedSessionId(i64);

/// Why an id could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a normalised session id must not be negative")]
pub struct NegativeNormalisedSessionId;

impl NormalisedSessionId {
    /// The first id a store assigns. `AUTOINCREMENT` starts at one.
    pub const FIRST: Self = Self(1);

    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl TryFrom<i64> for NormalisedSessionId {
    type Error = NegativeNormalisedSessionId;

    fn try_from(id: i64) -> Result<Self, Self::Error> {
        if id < 0 {
            return Err(NegativeNormalisedSessionId);
        }
        Ok(Self(id))
    }
}

impl fmt::Display for NormalisedSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// How many canonical sessions a join wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct SessionCount(usize);

impl SessionCount {
    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl From<usize> for SessionCount {
    fn from(count: usize) -> Self {
        Self(count)
    }
}

impl fmt::Display for SessionCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
