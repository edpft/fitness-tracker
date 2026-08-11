//! The names a landing record carries: what served it, what was called, and
//! what the source called the thing.

use std::fmt;

/// Why an identifier could not be constructed.
///
/// One enum rather than one per type: the failure modes are shared, and the
/// field name is what makes a message useful.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidIdentifier {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },
    #[error("{field} must not contain whitespace")]
    ContainsWhitespace { field: &'static str },
    #[error("{field} must be lowercase")]
    NotLowercase { field: &'static str },
    #[error("{field} must begin with '/'")]
    NotAbsolutePath { field: &'static str },
}

fn reject_empty(field: &'static str, value: &str) -> Result<(), InvalidIdentifier> {
    if value.is_empty() {
        return Err(InvalidIdentifier::Empty { field });
    }
    Ok(())
}

fn reject_whitespace(field: &'static str, value: &str) -> Result<(), InvalidIdentifier> {
    if value.chars().any(char::is_whitespace) {
        return Err(InvalidIdentifier::ContainsWhitespace { field });
    }
    Ok(())
}

fn reject_uppercase(field: &'static str, value: &str) -> Result<(), InvalidIdentifier> {
    if value.chars().any(char::is_uppercase) {
        return Err(InvalidIdentifier::NotLowercase { field });
    }
    Ok(())
}

/// The system that served an observation. `hevy`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceName(String);

impl SourceName {
    /// # Errors
    ///
    /// Returns [`InvalidIdentifier`] if the name is empty, contains
    /// whitespace, or is not lowercase.
    pub fn new(name: impl Into<String>) -> Result<Self, InvalidIdentifier> {
        let name = name.into();
        reject_empty("a source name", &name)?;
        reject_whitespace("a source name", &name)?;
        reject_uppercase("a source name", &name)?;
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The kind of thing a source serves us. `workouts`.
///
/// Paired with a [`SourceName`] it names a [`LandingStream`], which is the
/// unit that resumes, runs and locks independently.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityKind(String);

impl EntityKind {
    /// # Errors
    ///
    /// Returns [`InvalidIdentifier`] if the kind is empty, contains
    /// whitespace, or is not lowercase.
    pub fn new(kind: impl Into<String>) -> Result<Self, InvalidIdentifier> {
        let kind = kind.into();
        reject_empty("an entity kind", &kind)?;
        reject_whitespace("an entity kind", &kind)?;
        reject_uppercase("an entity kind", &kind)?;
        Ok(Self(kind))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
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

impl fmt::Display for LandingStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.source, self.entity)
    }
}

/// What was called to obtain a payload. `/v1/workouts/events`.
///
/// Real provenance rather than a constant: the same entity can arrive from
/// more than one endpoint of the same source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Endpoint(String);

impl Endpoint {
    /// # Errors
    ///
    /// Returns [`InvalidIdentifier`] if the endpoint is empty, contains
    /// whitespace, or does not begin with `/`.
    pub fn new(endpoint: impl Into<String>) -> Result<Self, InvalidIdentifier> {
        let endpoint = endpoint.into();
        reject_empty("an endpoint", &endpoint)?;
        reject_whitespace("an endpoint", &endpoint)?;
        if !endpoint.starts_with('/') {
            return Err(InvalidIdentifier::NotAbsolutePath {
                field: "an endpoint",
            });
        }
        Ok(Self(endpoint))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The identifier by which the source names this record.
///
/// Deliberately **not** parsed as a UUID, even though Hevy serves UUIDs and
/// says so in its published interface. Validating a source's identifier format
/// is interpreting a source field, which raw landing does not do — and it
/// would fail extraction to defend a constraint we do not own. Non-empty is
/// ours to require: provenance has no meaning without it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceRecordId(String);

impl SourceRecordId {
    /// # Errors
    ///
    /// Returns [`InvalidIdentifier`] if the identifier is empty or contains
    /// whitespace.
    pub fn new(id: impl Into<String>) -> Result<Self, InvalidIdentifier> {
        let id = id.into();
        reject_empty("a source record id", &id)?;
        reject_whitespace("a source record id", &id)?;
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SourceRecordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
