//! What a landing record is about: which stream it belongs to, and what the
//! source calls the thing it describes.

use std::fmt;

use super::newtype::string_name;

/// What separates a stream's two halves in its text form.
pub const STREAM_SEPARATOR: char = '.';

/// Why a name we assign could not be constructed.
///
/// One enum rather than one per type: these three types answer to the same
/// rules for the same reason, and the field name is what makes a message
/// useful. A name a *source* assigns is [`SourceRecordId`], which answers to
/// almost none of them.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidIdentifier {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} must not contain whitespace")]
    ContainsWhitespace { field: &'static str },
    #[error("{field} must be lowercase")]
    NotLowercase { field: &'static str },
    #[error("{field} must not contain '.'")]
    ContainsSeparator { field: &'static str },
}

/// The rules a name we assign must satisfy, and where they come from.
///
/// They are not a guess at what is generally reasonable. A [`LandingStream`]
/// is written `hevy.workouts` — on the command line, in a resumption point's
/// key, and in every message an operator reads — and that text form has to
/// round-trip. A separator inside a half would make the split ambiguous;
/// whitespace would make the argument need quoting; mixed case would let one
/// stream be spelled two ways and resume from two different places.
///
/// Nothing here is imposed on a value a source owns.
fn reject_unusable_name(field: &'static str, value: &str) -> Result<(), InvalidIdentifier> {
    if value.is_empty() {
        return Err(InvalidIdentifier::Empty { field });
    }
    if value.chars().any(char::is_whitespace) {
        return Err(InvalidIdentifier::ContainsWhitespace { field });
    }
    if value.chars().any(char::is_uppercase) {
        return Err(InvalidIdentifier::NotLowercase { field });
    }
    if value.contains(STREAM_SEPARATOR) {
        return Err(InvalidIdentifier::ContainsSeparator { field });
    }
    Ok(())
}

/// The system that served an observation. `hevy`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceName(String);

impl TryFrom<String> for SourceName {
    type Error = InvalidIdentifier;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        reject_unusable_name("a source name", &name)?;
        Ok(Self(name))
    }
}

string_name!(SourceName, InvalidIdentifier);

/// The kind of thing a source serves us. `workouts`.
///
/// Paired with a [`SourceName`] it names a [`LandingStream`], which is the
/// unit that resumes, runs and locks independently.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityKind(String);

impl TryFrom<String> for EntityKind {
    type Error = InvalidIdentifier;

    fn try_from(kind: String) -> Result<Self, Self::Error> {
        reject_unusable_name("an entity kind", &kind)?;
        Ok(Self(kind))
    }
}

string_name!(EntityKind, InvalidIdentifier);

/// Why a stream could not be read from its text form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidStream {
    #[error("a stream is named source.entity, and {value:?} is not")]
    Malformed { value: String },
    #[error(transparent)]
    Half(#[from] InvalidIdentifier),
}

/// One source's one entity type: `hevy.workouts`.
///
/// Extraction resumes, runs and locks per stream rather than per source, so
/// collecting Hevy workouts and Hevy body measurements never wait on each
/// other or share a resumption point.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LandingStream {
    source: SourceName,
    entity: EntityKind,
}

impl LandingStream {
    pub fn new(source: SourceName, entity: EntityKind) -> Self {
        Self { source, entity }
    }

    pub fn source(&self) -> &SourceName {
        &self.source
    }

    pub fn entity(&self) -> &EntityKind {
        &self.entity
    }
}

/// The inverse of [`LandingStream`]'s `Display`, which is what makes the name
/// an operator types the same name the system prints back.
impl TryFrom<&str> for LandingStream {
    type Error = InvalidStream;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let (source, entity) =
            value
                .split_once(STREAM_SEPARATOR)
                .ok_or_else(|| InvalidStream::Malformed {
                    value: value.to_owned(),
                })?;

        Ok(Self::new(
            SourceName::try_from(source)?,
            EntityKind::try_from(entity)?,
        ))
    }
}

impl std::str::FromStr for LandingStream {
    type Err = InvalidStream;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value)
    }
}

impl fmt::Display for LandingStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{STREAM_SEPARATOR}{}", self.source, self.entity)
    }
}

/// The identifier by which the source names this record.
///
/// Deliberately **not** parsed as a UUID, even though Hevy serves UUIDs and
/// says so in its published interface. Validating a source's identifier format
/// is interpreting a source field, which raw landing does not do — and it
/// would fail extraction to defend a constraint we do not own. Non-empty is
/// the one thing we do require, and it is required of us rather than of the
/// source: provenance has no meaning without it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceRecordId(String);

impl TryFrom<String> for SourceRecordId {
    type Error = InvalidIdentifier;

    fn try_from(id: String) -> Result<Self, Self::Error> {
        if id.is_empty() {
            return Err(InvalidIdentifier::Empty {
                field: "a source record id",
            });
        }
        Ok(Self(id))
    }
}

string_name!(SourceRecordId, InvalidIdentifier);
