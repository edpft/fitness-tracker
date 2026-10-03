//! A gym session as one source recorded it, in whichever shape that source's
//! records take.
//!
//! Sibling of [`super::canonical`] one layer down, and the input the canonical
//! layer matches and merges. Three shapes rather than one, because the sources
//! record three different kinds of thing and § II.3 keeps a source's shape out
//! of the entity: a log of what the operator typed, a log he wrote by hand with
//! no clock on it, and a watch's recording whose exercise names are its
//! classifier's guesses.
//!
//! **The arms are shapes, not sources.** Beyond The White Board and the
//! spreadsheets are two sources and one shape — a dated log — so they are one
//! arm. A fourth source that records what a log records adds no arm, which is
//! the test § II.3 sets: the entity is ours, and a source is translated into it.

use crate::canonical::Occurred;

use super::{
    manual::ManualGymSession, measured::MeasuredGymSession, performed::PerformedGymSession,
};

/// One normalised account of a gym session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalisedGymSession {
    /// A log that timestamps what it records: Hevy.
    Performed(PerformedGymSession),
    /// A log the operator wrote himself, dated to a day and no finer: the
    /// gym's export and the historical spreadsheets.
    Logged(ManualGymSession),
    /// A watch's recording: its heart rate, and the sets it classified itself.
    Measured(MeasuredGymSession),
}

impl NormalisedGymSession {
    /// When it happened, at the precision its source knew it to.
    ///
    /// The matching key, and the reason [`Occurred`] is declared at the
    /// canonical layer rather than here: a day and an instant are two
    /// precisions of one fact, and matching has to compare across them.
    pub fn occurred(&self) -> Occurred {
        match self {
            Self::Performed(session) => Occurred::At(session.started_at().clone()),
            Self::Logged(session) => Occurred::On(session.on()),
            Self::Measured(session) => Occurred::At(session.started_at().clone()),
        }
    }

    /// Whether the operator himself recorded what this session holds.
    ///
    /// **The one distinction the merge turns on.** A log is the operator's
    /// account of what he did; a watch's is its sensors' and its classifier's.
    /// Where the two disagree about a value they both recorded, which of them
    /// could have known it is the question, and nothing finer than this
    /// answers it — two logs disagreeing is settled by what a third account
    /// corroborates, not by which source they came from.
    pub const fn is_logged(&self) -> bool {
        match self {
            Self::Performed(_) | Self::Logged(_) => true,
            Self::Measured(_) => false,
        }
    }
}
