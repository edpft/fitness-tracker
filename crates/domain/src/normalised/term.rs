//! The source's own word for something inside one of its records.

use crate::landing::InvalidIdentifier;
use crate::newtype::string_name;

/// The source's own word for something inside one of its records.
///
/// Opaque here, and deliberately: for a Hevy set it is the exercise's
/// template and the set's index, for a watch's the instant it states the set
/// began, for a sheet's its cell. Which it is belongs to the adapter that reads
/// it (§ 8). What the domain needs is that a canonical set can name the source
/// sets it was built from (#893), not that it can interpret them.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceTerm(String);

impl TryFrom<String> for SourceTerm {
    type Error = InvalidIdentifier;

    fn try_from(term: String) -> Result<Self, Self::Error> {
        if term.is_empty() {
            return Err(InvalidIdentifier::Empty {
                field: "a source's term",
            });
        }
        Ok(Self(term))
    }
}

string_name!(SourceTerm, InvalidIdentifier);
