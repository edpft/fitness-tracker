//! When something started, in a way that survives a time zone.
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

/// When an observation started.
///
/// There is no constructor taking an instant alone, which is how "never naive"
/// stops being a rule to remember and becomes a shape the type will not let you
/// build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartedAt {
    instant: Timestamp,
    zone: OperatorZone,
}

impl StartedAt {
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

impl fmt::Display for StartedAt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.wall_clock())
    }
}

/// Every zone the operator has stated, each in force from when it was stated
/// until the next (#66).
///
/// § 13: the value in force at the time of the observation is the one that
/// applies. Translating every observation through the latest zone instead
/// would move every past session's wall clock the day the operator moves
/// house.
///
/// **The first zone stated is in force before it was stated**, because the
/// record reaches back years before anyone stated one, and the alternative is
/// a record with no wall clock at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatedZones {
    first: OperatorZone,
    /// Each later zone and the instant it was stated, oldest first.
    later: Vec<(Timestamp, OperatorZone)>,
}

impl StatedZones {
    /// One zone, in force throughout.
    #[must_use]
    pub const fn throughout(zone: OperatorZone) -> Self {
        Self {
            first: zone,
            later: Vec::new(),
        }
    }

    /// The zones stated, each with when; `None` if there are none.
    #[must_use]
    pub fn stated(statements: impl IntoIterator<Item = (Timestamp, OperatorZone)>) -> Option<Self> {
        let mut statements: Vec<_> = statements.into_iter().collect();
        statements.sort_by_key(|(at, _)| *at);
        let mut statements = statements.into_iter();
        let (_, first) = statements.next()?;
        Some(Self {
            first,
            later: statements.collect(),
        })
    }

    /// The zone in force at `instant`.
    #[must_use]
    pub fn in_force_at(&self, instant: Timestamp) -> &OperatorZone {
        self.later
            .iter()
            .rev()
            .find(|(from, _)| *from <= instant)
            .map_or(&self.first, |(_, zone)| zone)
    }

    /// When something started at `instant`, in the zone in force then.
    #[must_use]
    pub fn started_at(&self, instant: Timestamp) -> StartedAt {
        StartedAt::new(instant, self.in_force_at(instant).clone())
    }
}

/// The first zone alone, then each later one with when it was stated — so one
/// zone reads as its identifier, as it did before there could be more.
impl fmt::Display for StatedZones {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.first)?;
        for (from, zone) in &self.later {
            write!(f, ", {zone} from {from}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use super::{OperatorZone, StatedZones};

    fn at(text: &str) -> Timestamp {
        text.parse().expect("an instant")
    }

    fn zone(id: &str) -> OperatorZone {
        OperatorZone::try_from(id).expect("a zone")
    }

    fn moved() -> StatedZones {
        StatedZones::stated([
            (at("2027-03-01T09:00:00Z"), zone("Pacific/Auckland")),
            (at("2026-09-15T13:29:25Z"), zone("Europe/London")),
        ])
        .expect("two zones")
    }

    #[test]
    fn a_session_before_the_move_keeps_its_wall_clock() {
        let started = moved().started_at(at("2026-10-20T17:00:00Z"));
        assert_eq!(started.wall_clock().time().to_string(), "18:00:00");
        assert_eq!(started.wall_clock().date().to_string(), "2026-10-20");
    }

    #[test]
    fn a_session_after_the_move_reads_in_the_new_zone() {
        let started = moved().started_at(at("2027-03-02T05:00:00Z"));
        assert_eq!(started.zone().id(), "Pacific/Auckland");
        assert_eq!(started.wall_clock().time().to_string(), "18:00:00");
    }

    #[test]
    fn the_first_zone_reaches_back_before_it_was_stated() {
        assert_eq!(
            moved().in_force_at(at("2016-05-01T07:00:00Z")).id(),
            "Europe/London"
        );
    }

    #[test]
    fn nothing_stated_is_no_zones() {
        assert_eq!(StatedZones::stated([]), None);
    }

    #[test]
    fn one_zone_reads_as_its_identifier() {
        assert_eq!(
            StatedZones::throughout(zone("Europe/London")).to_string(),
            "Europe/London"
        );
        assert_eq!(
            moved().to_string(),
            "Europe/London, Pacific/Auckland from 2027-03-01T09:00:00Z"
        );
    }
}
