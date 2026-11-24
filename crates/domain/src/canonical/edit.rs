//! What every edit of an account is, whatever it edits (§ II.2): an
//! assertion the operator made, and why.
//!
//! A gym session's edit ([`crate::gym::edit`]) and a padel session's
//! ([`crate::padel::PadelEdit`]) each say what they change; this is what they
//! share.

use std::fmt;

use crate::landing::InvalidIdentifier;
use crate::newtype::string_name;

/// What the store calls one assertion.
///
/// A surrogate, and allowed to be: it identifies the *assertion* so the
/// operator can retract it as he made it, and nothing is anchored by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EditId(i64);

impl From<i64> for EditId {
    fn from(id: i64) -> Self {
        Self(id)
    }
}

impl EditId {
    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl fmt::Display for EditId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Why the operator made an edit.
///
/// Required, because § II.2 says an unexplained override is unreadable six
/// months later. Non-empty is the whole of the validation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EditReason(String);

impl TryFrom<String> for EditReason {
    type Error = InvalidIdentifier;

    fn try_from(reason: String) -> Result<Self, Self::Error> {
        if reason.trim().is_empty() {
            return Err(InvalidIdentifier::Empty {
                field: "an edit's reason",
            });
        }
        Ok(Self(reason))
    }
}

string_name!(EditReason, InvalidIdentifier);
