//! What a source said happened to a record it serves.

use std::fmt;

use crate::newtype::string_name;

use super::ids::InvalidIdentifier;

/// A kind the source used that we do not recognise.
///
/// Kept verbatim. Normalising it would be interpretation, and comparing it
/// against a list we control would make the source's vocabulary ours.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RawEventKind(String);

impl TryFrom<String> for RawEventKind {
    type Error = InvalidIdentifier;

    fn try_from(kind: String) -> Result<Self, Self::Error> {
        if kind.is_empty() {
            return Err(InvalidIdentifier::Empty {
                field: "an event kind",
            });
        }
        Ok(Self(kind))
    }
}

string_name!(RawEventKind, InvalidIdentifier);

/// What the source said happened.
///
/// The `Unrecognised` variant is deliberate rather than defensive. A kind the
/// source adds next year is *unknown*, not *illegal*, and raw landing retains
/// what it does not recognise instead of discarding it. Modelling it as a
/// variant is what lets that hold without the type claiming to have understood
/// something it has not — while `Updated` and `Deleted` stay distinguishable,
/// so no caller can confuse the two that carry meaning.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventKind {
    Updated,
    Deleted,
    Unrecognised(RawEventKind),
}

impl EventKind {
    /// The kind as the source expressed it, which is also what gets stored.
    ///
    /// Round-trips through `TryFrom<&str>`.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Updated => "updated",
            Self::Deleted => "deleted",
            Self::Unrecognised(kind) => kind.as_str(),
        }
    }
}

impl TryFrom<String> for EventKind {
    type Error = InvalidIdentifier;

    fn try_from(kind: String) -> Result<Self, Self::Error> {
        match kind.as_str() {
            "updated" => Ok(Self::Updated),
            "deleted" => Ok(Self::Deleted),
            _ => RawEventKind::try_from(kind).map(Self::Unrecognised),
        }
    }
}

impl TryFrom<&str> for EventKind {
    type Error = InvalidIdentifier;

    fn try_from(kind: &str) -> Result<Self, Self::Error> {
        Self::try_from(kind.to_owned())
    }
}

impl std::str::FromStr for EventKind {
    type Err = InvalidIdentifier;

    fn from_str(kind: &str) -> Result<Self, Self::Err> {
        Self::try_from(kind.to_owned())
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
