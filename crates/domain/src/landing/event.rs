//! What kind of event produced a landing record.

use std::fmt;

use super::ids::InvalidIdentifier;

/// A kind the source used that we do not recognise.
///
/// Kept verbatim. Normalising it would be interpretation, and comparing it
/// against a list we control would make the source's vocabulary ours.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RawEventKind(String);

impl RawEventKind {
    /// # Errors
    ///
    /// Returns [`InvalidIdentifier`] if the kind is empty.
    pub fn new(kind: impl Into<String>) -> Result<Self, InvalidIdentifier> {
        let kind = kind.into();
        if kind.is_empty() {
            return Err(InvalidIdentifier::Empty {
                field: "an event kind",
            });
        }
        Ok(Self(kind))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RawEventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

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
    /// Read a kind as the source expressed it.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidIdentifier`] if the source supplied an empty kind.
    pub fn from_source(kind: &str) -> Result<Self, InvalidIdentifier> {
        match kind {
            "updated" => Ok(Self::Updated),
            "deleted" => Ok(Self::Deleted),
            other => Ok(Self::Unrecognised(RawEventKind::new(other)?)),
        }
    }

    /// The kind as the source expressed it, which is what gets stored.
    pub fn as_source_str(&self) -> &str {
        match self {
            Self::Updated => "updated",
            Self::Deleted => "deleted",
            Self::Unrecognised(kind) => kind.as_str(),
        }
    }

    /// Whether this event asserts the record no longer exists at the source.
    pub fn is_deletion(&self) -> bool {
        matches!(self, Self::Deleted)
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_source_str())
    }
}
