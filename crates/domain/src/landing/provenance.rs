//! How a payload reached us.
//!
//! Separate from the record itself because it is the part that differs by
//! transport. What every record carries — which stream, which source record,
//! when we fetched it, the bytes — is the same whatever served it, and lives
//! in [`super::record::LandingRecord`].

use std::fmt;

use crate::newtype::string_name;

use super::{
    event::EventKind,
    time::{EventTime, ModifiedAt},
};

/// Why an endpoint could not be constructed.
///
/// Its own error rather than a shared one: an endpoint is a path, and "must
/// begin with `/`" is a rule about paths that no name answers to.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidEndpoint {
    #[error("an endpoint must not be empty")]
    Empty,
    #[error("an endpoint must not contain whitespace")]
    ContainsWhitespace,
    #[error("an endpoint must begin with '/'")]
    NotAbsolutePath,
}

/// What was called to obtain a payload. `/v1/workouts/events`.
///
/// Real provenance rather than a constant: the same entity can arrive from
/// more than one endpoint of the same source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Endpoint(String);

impl TryFrom<String> for Endpoint {
    type Error = InvalidEndpoint;

    fn try_from(endpoint: String) -> Result<Self, Self::Error> {
        if endpoint.is_empty() {
            return Err(InvalidEndpoint::Empty);
        }
        if endpoint.chars().any(char::is_whitespace) {
            return Err(InvalidEndpoint::ContainsWhitespace);
        }
        if !endpoint.starts_with('/') {
            return Err(InvalidEndpoint::NotAbsolutePath);
        }
        Ok(Self(endpoint))
    }
}

string_name!(Endpoint, InvalidEndpoint);

/// What an HTTP feed of change events knows about a record it served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventProvenance {
    endpoint: Endpoint,
    kind: EventKind,
    /// When the source says the event happened.
    ///
    /// Optional: a source is free to serve an event without one, and
    /// substituting the fetch time would be inventing a fact — as well as
    /// risking a resumption point that steps over events never seen.
    occurred_at: Option<EventTime>,
}

impl EventProvenance {
    pub const fn new(endpoint: Endpoint, kind: EventKind, occurred_at: Option<EventTime>) -> Self {
        Self {
            endpoint,
            kind,
            occurred_at,
        }
    }

    pub const fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub const fn kind(&self) -> &EventKind {
        &self.kind
    }

    pub const fn occurred_at(&self) -> Option<EventTime> {
        self.occurred_at
    }
}

/// Why a file's path could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidFilePath {
    #[error("a file path must not be empty")]
    Empty,
    #[error(
        "a file path is relative to the folder it was landed from, and must not begin with '/'"
    )]
    Absolute,
    #[error("a file path must not contain an empty, '.' or '..' component")]
    NotNormal,
}

/// Where a file sat, relative to the folder it was landed from.
/// `Dropbox/Random/Training 2014.xlsx`.
///
/// Relative, because the folder is wherever the operator put it this time and
/// is no part of what the file is. Separated by `/` whatever the platform, so
/// the same file lands with the same path from any machine.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FilePath(String);

impl TryFrom<String> for FilePath {
    type Error = InvalidFilePath;

    fn try_from(path: String) -> Result<Self, Self::Error> {
        if path.is_empty() {
            return Err(InvalidFilePath::Empty);
        }
        if path.starts_with('/') {
            return Err(InvalidFilePath::Absolute);
        }
        if path
            .split('/')
            .any(|component| matches!(component, "" | "." | ".."))
        {
            return Err(InvalidFilePath::NotNormal);
        }
        Ok(Self(path))
    }
}

string_name!(FilePath, InvalidFilePath);

/// What a folder of files knows about one it held: where, and when it was last
/// saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProvenance {
    path: FilePath,
    modified_at: ModifiedAt,
}

impl FileProvenance {
    pub const fn new(path: FilePath, modified_at: ModifiedAt) -> Self {
        Self { path, modified_at }
    }

    pub const fn path(&self) -> &FilePath {
        &self.path
    }

    pub const fn modified_at(&self) -> ModifiedAt {
        self.modified_at
    }
}

/// How a payload reached us, in the terms the thing that carried it has.
///
/// An enum rather than a widening of [`super::record::LandingRecord`]'s own
/// fields, so that a second transport is an addition instead of a rewrite: a
/// file has no endpoint, no event kind and no event time, and handing it empty
/// ones would be recording facts we do not have.
///
/// What a new variant may not do is add to what *every* record carries. That
/// core is the record's, and it is the same whatever served it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// Served by an HTTP feed of change events.
    Event(EventProvenance),
    /// Read from a folder of files the operator put together.
    File(FileProvenance),
}

impl Provenance {
    /// When the source says the thing happened, if it says.
    ///
    /// The one question about provenance that is worth asking without knowing
    /// what carried the payload: it is where a resumption point comes from,
    /// and every transport has some answer to it — including "none", which is
    /// why it is optional rather than absent.
    ///
    /// A file's answer is none. Its modification time says when it was last
    /// saved, not when anything in it happened, and a folder has no feed to
    /// resume.
    pub const fn occurred_at(&self) -> Option<EventTime> {
        match self {
            Self::Event(event) => event.occurred_at(),
            Self::File(_) => None,
        }
    }

    /// The feed's account, if a feed served it.
    pub const fn as_event(&self) -> Option<&EventProvenance> {
        match self {
            Self::Event(event) => Some(event),
            Self::File(_) => None,
        }
    }

    /// The folder's account, if it was read from one.
    pub const fn as_file(&self) -> Option<&FileProvenance> {
        match self {
            Self::File(file) => Some(file),
            Self::Event(_) => None,
        }
    }
}

impl From<EventProvenance> for Provenance {
    fn from(event: EventProvenance) -> Self {
        Self::Event(event)
    }
}

impl From<FileProvenance> for Provenance {
    fn from(file: FileProvenance) -> Self {
        Self::File(file)
    }
}

impl fmt::Display for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Event(event) => write!(f, "{} {}", event.kind(), event.endpoint()),
            Self::File(file) => write!(f, "file {}", file.path()),
        }
    }
}
