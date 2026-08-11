//! Instants.
//!
//! Raw landing deals only in instants, so everything here wraps a
//! `jiff::Timestamp`. Zone handling arrives with the normalised layer, where
//! wall-clock time starts to matter; the distinction is why the library was
//! chosen, and why there is no naive local type in reach.

use std::{fmt, str::FromStr};

use jiff::Timestamp;

/// Why an instant could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not a valid RFC 3339 timestamp")]
pub struct InvalidTimestamp {
    value: String,
}

fn parse(value: &str) -> Result<Timestamp, InvalidTimestamp> {
    Timestamp::from_str(value).map_err(|_| InvalidTimestamp {
        value: value.to_owned(),
    })
}

/// When the fetch that produced a record ran.
///
/// Ours, not the source's — it says when we asked, which is what makes a
/// record's age answerable independently of anything the source claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FetchedAt(Timestamp);

impl FetchedAt {
    pub fn new(at: Timestamp) -> Self {
        Self(at)
    }

    /// # Errors
    ///
    /// Returns [`InvalidTimestamp`] if the value is not RFC 3339.
    pub fn parse(value: &str) -> Result<Self, InvalidTimestamp> {
        parse(value).map(Self)
    }

    pub fn as_timestamp(&self) -> Timestamp {
        self.0
    }
}

impl fmt::Display for FetchedAt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// When the source says the event happened.
///
/// Optional on a landing record: a source is free to serve an event without
/// one, and substituting the fetch time would be inventing a fact — as well as
/// risking a resumption point that steps over events never seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventTime(Timestamp);

impl EventTime {
    pub fn new(at: Timestamp) -> Self {
        Self(at)
    }

    /// # Errors
    ///
    /// Returns [`InvalidTimestamp`] if the value is not RFC 3339.
    pub fn parse(value: &str) -> Result<Self, InvalidTimestamp> {
        parse(value).map(Self)
    }

    pub fn as_timestamp(&self) -> Timestamp {
        self.0
    }
}

impl fmt::Display for EventTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Where extraction resumes from.
///
/// Reconstructible state: losing it costs a re-fetch, never a fact. It is
/// derived from event times a run actually observed — never from the clock,
/// which is the invariant that keeps a concurrent edit from being stepped
/// over. See [`crate::landing::run`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Watermark(Timestamp);

impl Watermark {
    pub fn new(at: Timestamp) -> Self {
        Self(at)
    }

    /// # Errors
    ///
    /// Returns [`InvalidTimestamp`] if the value is not RFC 3339.
    pub fn parse(value: &str) -> Result<Self, InvalidTimestamp> {
        parse(value).map(Self)
    }

    pub fn as_timestamp(&self) -> Timestamp {
        self.0
    }

    /// The later of two positions.
    ///
    /// How a run accumulates its resumption point: fold every event time seen,
    /// keeping the newest.
    #[must_use]
    pub fn advanced_to(self, event: EventTime) -> Self {
        if event.as_timestamp() > self.0 {
            Self(event.as_timestamp())
        } else {
            self
        }
    }
}

impl From<EventTime> for Watermark {
    fn from(event: EventTime) -> Self {
        Self(event.as_timestamp())
    }
}

impl fmt::Display for Watermark {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
