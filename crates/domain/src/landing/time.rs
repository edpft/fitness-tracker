//! Instants.
//!
//! Raw landing deals only in instants, so everything here wraps a
//! `jiff::Timestamp`. Zone handling arrives with the normalised layer, where
//! wall-clock time starts to matter; the distinction is why the library was
//! chosen, and why there is no naive local type in reach.

use std::str::FromStr;

use jiff::Timestamp;

use crate::newtype::instant;

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
    /// The Unix epoch. Where a stream that has never run begins.
    pub const EPOCH: Self = Self(Timestamp::UNIX_EPOCH);
}

instant!(FetchedAt);

/// When the source says the event happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventTime(Timestamp);

instant!(EventTime);

/// Where extraction resumes from.
///
/// Reconstructible state: losing it costs a re-fetch, never a fact. It is
/// derived from event times a run actually observed — never from the clock,
/// which is the invariant that keeps a concurrent edit from being stepped
/// over. See [`crate::landing::run`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Watermark(Timestamp);

impl Watermark {
    /// The Unix epoch, which is also the source's own default for `since`.
    pub const EPOCH: Self = Self(Timestamp::UNIX_EPOCH);

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

instant!(Watermark);

impl From<EventTime> for Watermark {
    fn from(event: EventTime) -> Self {
        Self(event.as_timestamp())
    }
}
