//! When a workout started, in a way that survives a time zone.
//!
//! § II.3: timestamps carry an IANA identifier and are never naive. 8pm stays
//! 8pm — wall-clock time is what is entered and what is displayed. Which
//! physical encoding carries it is an implementation choice, because given the
//! zone the two forms are losslessly interconvertible; this one stores the
//! instant and the zone, because the source supplies an instant and storing it
//! unchanged means nothing is decided at write time that a zone correction
//! could not undo.
//!
//! An offset is not a substitute. It records the rule that applied at one
//! instant, not the rule that applies across an interval — so arithmetic and
//! calendar bucketing resolve through the zone here, and a system that assumes
//! every local day is 24 hours long is wrong twice a year.

use std::fmt;

use jiff::{Timestamp, Zoned, tz::TimeZone};

/// Why a zone could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not an IANA time zone identifier")]
pub struct UnknownTimeZone {
    value: String,
}

/// The zone the operator declares they train in.
///
/// A versioned input to deterministic translation (§ 9), not an inference about
/// the source or about the data. Travel is invisible in a workout payload and
/// is an edit-overlay correction, which is why nothing here tries to detect it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorZone {
    id: String,
    zone: TimeZone,
}

impl OperatorZone {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub(crate) const fn zone(&self) -> &TimeZone {
        &self.zone
    }

    /// The zone itself, for a caller that has to do calendar arithmetic.
    ///
    /// Public where [`Self::zone`] is not, because a prescription is placed on a
    /// calendar day and the ring that does the placing is outside this crate.
    #[must_use]
    pub fn as_time_zone(&self) -> TimeZone {
        self.zone.clone()
    }
}

impl TryFrom<String> for OperatorZone {
    type Error = UnknownTimeZone;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let zone = TimeZone::get(&value).map_err(|_| UnknownTimeZone {
            value: value.clone(),
        })?;
        Ok(Self { id: value, zone })
    }
}

impl fmt::Display for OperatorZone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id)
    }
}

crate::newtype::from_str_via_string!(OperatorZone, UnknownTimeZone);

/// When a workout started.
///
/// There is no constructor taking an instant alone, which is how "never naive"
/// stops being a rule to remember and becomes a shape the type will not let you
/// build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkoutStart {
    instant: Timestamp,
    zone: OperatorZone,
}

impl WorkoutStart {
    pub const fn new(instant: Timestamp, zone: OperatorZone) -> Self {
        Self { instant, zone }
    }

    pub const fn instant(&self) -> Timestamp {
        self.instant
    }

    pub const fn zone(&self) -> &OperatorZone {
        &self.zone
    }

    /// The local time that was trained at.
    ///
    /// Resolved through the zone rather than through a stored offset, so a
    /// session at 6pm reads as 6pm on both sides of a switchover.
    pub fn wall_clock(&self) -> Zoned {
        self.instant.to_zoned(self.zone.zone().clone())
    }
}

impl fmt::Display for WorkoutStart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.wall_clock())
    }
}
