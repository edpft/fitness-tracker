//! How a payload reached us.
//!
//! Separate from the record itself because it is the part that differs by
//! transport. What every record carries — which stream, which source record,
//! when we fetched it, the bytes — is the same whatever served it, and lives
//! in [`super::record::LandingRecord`].

use std::fmt;

use super::{event::EventKind, newtype::string_name, time::EventTime};

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

/// How a payload reached us, in the terms the thing that carried it has.
///
/// One variant, because one transport has been built. It is an enum rather
/// than a widening of [`super::record::LandingRecord`]'s own fields so that
/// the second transport is an addition instead of a rewrite: a CSV export has
/// no endpoint, no event kind and no event time, and handing it empty ones
/// would be recording facts we do not have.
///
/// What a new variant may not do is add to what *every* record carries. That
/// core is the record's, and it is the same whatever served it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// Served by an HTTP feed of change events.
    Event(EventProvenance),
}

impl Provenance {
    /// When the source says the thing happened, if it says.
    ///
    /// The one question about provenance that is worth asking without knowing
    /// what carried the payload: it is where a resumption point comes from,
    /// and every transport has some answer to it — including "none", which is
    /// why it is optional rather than absent.
    pub const fn occurred_at(&self) -> Option<EventTime> {
        let Self::Event(event) = self;
        event.occurred_at()
    }
}

impl From<EventProvenance> for Provenance {
    fn from(event: EventProvenance) -> Self {
        Self::Event(event)
    }
}

impl fmt::Display for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self::Event(event) = self;
        write!(f, "{} {}", event.kind(), event.endpoint())
    }
}
