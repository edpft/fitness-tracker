//! One authored cycling mesocycle: its rides, and where they are ridden.
//!
//! **Four of these make the autumn's cycling programme** — a one-microcycle FTP
//! test, then three of four — and decision 0026 is why it is not one
//! thirteen-week record: a mesocycle taken from *Build Your Power Zones* and one
//! taken from *Peak Your Power Zones* are two published programmes with their
//! own vocabulary, and a single row spanning both would mix bounded contexts.
//!
//! **It was `CyclingProgramme` until 2026-09-06**, and the level was wrong. The
//! operator's hierarchy puts a *programme* above a mesocycle: the four of these
//! are the cycling programme, and what holds them together is the plan they
//! belong to rather than the successive start dates they used to be related by
//! (issue #86).
//!
//! **This is a record of intent (§ 12), so it holds what is ridden and not what
//! was offered.** A published mesocycle prescribes three sessions a microcycle
//! and the operator rides two; the two are here, and the published numbering on
//! each microcycle and ride keeps the way back to the rest.
//!
//! **The zone plan is stored rather than re-derived.** Re-fetching a class is
//! neither free nor always available, and § 13 wants a prescription issued last
//! month to stay reproducible — so the intervals are here in full and nothing
//! reads the network to answer what the next ride is.
//!
//! **A session's identifiers are the adapter's** (§ II.3). What crosses into
//! this crate is a [`RideVenue`]: an opaque reference the destination issued and
//! the name it gives it, in exactly the position
//! [`DeliveryReference`](crate::prescription::DeliveryReference) holds — neither
//! is interpreted here, and nothing here knows a Peloton class id exists.
//!
//! **Identity is the plan's, and there is no overlap rule here at all.** A
//! mesocycle had a [`ProgrammeName`] of its own until 2026-09-06 and competed
//! for its days against every other cycling mesocycle, which is how four of
//! them were held in sequence without a plan to hold them. `Programme` orders
//! them now and refuses two that collide, and the only overlap rule left is
//! between *plans* — so the gym and the bike compete for every day of one plan
//! on purpose, which is the point of the tool.
//!
//! **What is left of a name is the published one.** [`ExternalProgramme`] says
//! which Peloton programme these microcycles were taken from, and
//! [`CyclingMesocycle::provided_from`] pairs it with their published numbers.

use std::collections::BTreeMap;

use jiff::civil::{Date, Weekday};

use crate::{
    gym::sequence::NonEmpty,
    provider::{ExternalProgramme, InvalidProvision, ProvidedFrom},
};

use super::session::CyclingSession;

/// Which session of a microcycle, counting from one.
///
/// **This programme's own numbering, not the published one.** The operator
/// rides two of the three sessions a published microcycle states, and those two
/// are his first and second — the same rule the microcycles follow, where an
/// answer of µ1-2-4-5 authors a first, second, third and fourth. Which
/// published session each was is carried by
/// [`PlannedRide::published_session`], for the same reason
/// [`CyclingMicrocycle::published_ordinal`] exists: it is the way back to what
/// was not chosen.
///
/// Numbering them as published would make the printed line say "session 3" for
/// the second of two, which is what the operator caught on 2026-09-05.
///
/// **Ordinal, never a weekday** (decision 0018): a programme states a first and
/// a second session and says nothing about Wednesdays. What maps one onto a
/// calendar day is [`CyclingWeekdays`].
///
/// No ceiling beyond the type's own. How many sessions a microcycle holds is a
/// fact about a published programme rather than a number to decide here —
/// *Discover* runs seven in its first week — so nothing is refused that a
/// programme might really contain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionPosition(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a microcycle's sessions count from one, so there is no session {value}")]
pub struct InvalidSessionPosition {
    value: u8,
}

impl SessionPosition {
    /// # Errors
    ///
    /// [`InvalidSessionPosition`] for zero.
    pub const fn new(position: u8) -> Result<Self, InvalidSessionPosition> {
        if position == 0 {
            return Err(InvalidSessionPosition { value: position });
        }
        Ok(Self(position))
    }

    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

impl std::fmt::Display for SessionPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "session {}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidVenue {
    #[error("a ride has to be done somewhere, and an empty reference names nowhere")]
    EmptyReference,
    #[error("a destination that names a place calls it something")]
    EmptyName,
}

/// What a destination calls one place a ride is done.
///
/// Validated for emptiness and nothing else, exactly as
/// [`DeliveryReference`](crate::prescription::DeliveryReference) is: the
/// reference belongs to the system that issued it, and imposing a shape on it
/// would be this side inventing a rule the issuer never agreed to.
///
/// **The name is carried beside the reference** because it is what a
/// prescription prints. It identifies nothing — two Peloton classes really do
/// share a title — so nothing reads it but the report.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RideVenue {
    reference: String,
    called: String,
}

impl RideVenue {
    /// # Errors
    ///
    /// [`InvalidVenue`] if either half is empty once trimmed.
    pub fn new(reference: &str, called: &str) -> Result<Self, InvalidVenue> {
        let reference = reference.trim().to_owned();
        let called = called.trim().to_owned();
        if reference.is_empty() {
            return Err(InvalidVenue::EmptyReference);
        }
        if called.is_empty() {
            return Err(InvalidVenue::EmptyName);
        }
        Ok(Self { reference, called })
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }

    pub fn called(&self) -> &str {
        &self.called
    }
}

impl std::fmt::Display for RideVenue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.called)
    }
}

/// One ride, and where it is done.
///
/// **A session is one or more places** (decision 0033). The FTP warm-up and the
/// test itself are two classes and one session, so the venues are a sequence
/// and the [`CyclingSession`] beside them is the pair taken together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedRide {
    session: CyclingSession,
    at: NonEmpty<RideVenue>,
    published_session: u32,
}

impl PlannedRide {
    pub const fn new(
        session: CyclingSession,
        at: NonEmpty<RideVenue>,
        published_session: u32,
    ) -> Self {
        Self {
            session,
            at,
            published_session,
        }
    }

    pub const fn session(&self) -> &CyclingSession {
        &self.session
    }

    /// Every place this ride is done, in the order they are ridden.
    pub const fn at(&self) -> &NonEmpty<RideVenue> {
        &self.at
    }

    /// Which session of the published microcycle this was.
    ///
    /// The counterpart of [`CyclingMicrocycle::published_ordinal`], and kept for
    /// the same reason: a re-authoring that wants the session the operator did
    /// not take has to know which one it is asking the provider for.
    pub const fn published_session(&self) -> u32 {
        self.published_session
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidMicrocycle {
    #[error("a microcycle with no ride in it is a week off, not a microcycle")]
    NoRides,
    #[error("a published programme's microcycles count from one, so there is no microcycle 0")]
    ZeroPublishedOrdinal,
}

/// One week of an authored programme: what is ridden, and in what order.
///
/// Keyed on this programme's own session numbering, so the first entry is the
/// first ride of the week whichever published session it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CyclingMicrocycle {
    rides: BTreeMap<SessionPosition, PlannedRide>,
    published_ordinal: u32,
}

impl CyclingMicrocycle {
    /// # Errors
    ///
    /// [`InvalidMicrocycle::NoRides`] if it prescribes nothing.
    pub fn new(
        rides: BTreeMap<SessionPosition, PlannedRide>,
        published_ordinal: u32,
    ) -> Result<Self, InvalidMicrocycle> {
        if rides.is_empty() {
            return Err(InvalidMicrocycle::NoRides);
        }
        if published_ordinal == 0 {
            return Err(InvalidMicrocycle::ZeroPublishedOrdinal);
        }
        Ok(Self {
            rides,
            published_ordinal,
        })
    }

    pub const fn rides(&self) -> &BTreeMap<SessionPosition, PlannedRide> {
        &self.rides
    }

    #[must_use]
    pub fn ride(&self, at: SessionPosition) -> Option<&PlannedRide> {
        self.rides.get(&at)
    }

    /// Which microcycle of the published programme this is, in that
    /// programme's own numbering.
    ///
    /// An answer of µ1-2-4-5 keeps the four numbers the programme uses, so the
    /// third microcycle here says 4. Which programme they are microcycles *of*
    /// is [`CyclingMesocycle::programme`], said once rather than on every week.
    pub const fn published_ordinal(&self) -> u32 {
        self.published_ordinal
    }

    /// How many sessions the week holds. What "the second of two" counts
    /// against.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.rides.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidWeekdays {
    #[error("a programme that rides no day of the week prescribes nothing")]
    NoDays,
    #[error("{weekday} is given two sessions, and a day rides one")]
    RepeatedWeekday { weekday: String },
    #[error("{position} is ridden on two weekdays, and a session is ridden once")]
    RepeatedSession { position: SessionPosition },
}

/// Which weekday rides which session of the microcycle.
///
/// **Two facts in one place because they must agree.** Taking the week's last
/// session and riding on Sunday are the same decision — it is the long ride and
/// Sunday morning is the only slot long enough — and splitting them across two
/// records would let them drift.
///
/// **Fixed at authoring, as the gym's `Weekdays` are.** A programme's weekdays
/// are its weekly shape; an alteration is a run of days that departs from that
/// shape, and the calendar takes those out separately.
///
/// A list rather than a map because `jiff`'s `Weekday` is deliberately not
/// `Ord` — a week has no universal first day. At most a handful of entries, so
/// a scan is the whole cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CyclingWeekdays {
    days: Vec<(Weekday, SessionPosition)>,
}

impl CyclingWeekdays {
    /// # Errors
    ///
    /// [`InvalidWeekdays`] for an empty map, a weekday given two sessions, or a
    /// session given two weekdays.
    pub fn new(days: Vec<(Weekday, SessionPosition)>) -> Result<Self, InvalidWeekdays> {
        if days.is_empty() {
            return Err(InvalidWeekdays::NoDays);
        }
        for (at, (weekday, position)) in days.iter().enumerate() {
            let rest = days.iter().skip(at + 1);
            for (later_day, later_position) in rest {
                if later_day == weekday {
                    return Err(InvalidWeekdays::RepeatedWeekday {
                        weekday: format!("{weekday:?}"),
                    });
                }
                if later_position == position {
                    return Err(InvalidWeekdays::RepeatedSession {
                        position: *position,
                    });
                }
            }
        }
        Ok(Self { days })
    }

    #[must_use]
    pub fn days(&self) -> &[(Weekday, SessionPosition)] {
        &self.days
    }

    /// Which session this date rides, if it rides one.
    #[must_use]
    pub fn session_on(&self, date: Date) -> Option<SessionPosition> {
        self.days
            .iter()
            .find(|(weekday, _)| *weekday == date.weekday())
            .map(|(_, position)| *position)
    }

    /// Every session position the week rides, in order.
    #[must_use]
    pub fn positions(&self) -> Vec<SessionPosition> {
        let mut positions: Vec<SessionPosition> =
            self.days.iter().map(|(_, position)| *position).collect();
        positions.sort_unstable();
        positions
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidCyclingMesocycle {
    #[error("microcycle {microcycle} has no {position}, which the programme rides every week")]
    MissingRide {
        microcycle: usize,
        position: SessionPosition,
    },
    /// The published numbers are not a selection: one is taken twice.
    ///
    /// **Not reachable through [`CyclingMicrocycle::new`]**, which refuses a
    /// zero on its own — this is the rule that spans the weeks rather than one
    /// of them.
    #[error(transparent)]
    NotASelection(#[from] InvalidProvision),
}

/// A row identity, given by the store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CyclingMesocycleId(i64);

impl CyclingMesocycleId {
    pub const fn new(id: i64) -> Self {
        Self(id)
    }

    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for CyclingMesocycleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An authored cycling mesocycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CyclingMesocycle {
    /// The published programme these microcycles were taken from.
    ///
    /// **Always present on this side.** A cycling mesocycle is microcycles of a
    /// Peloton programme; there is no derived kind of one, which is why this is
    /// not an `Option` as the gym's provider is.
    programme: ExternalProgramme,
    /// The Monday microcycle one begins on.
    start: Date,
    microcycles: NonEmpty<CyclingMicrocycle>,
    weekdays: CyclingWeekdays,
}

impl CyclingMesocycle {
    /// # Errors
    ///
    /// [`InvalidCyclingMesocycle::MissingRide`] where a microcycle does not hold
    /// a session the weekday map rides. That is the one thing the parts cannot
    /// guarantee between them, and leaving it unchecked would author a
    /// programme with a Wednesday nothing answers for.
    ///
    /// [`InvalidCyclingMesocycle::NotASelection`] where two microcycles claim
    /// the same published week. A selection takes each week once, and µ1-2-2-4
    /// is not one anybody made.
    pub fn new(
        programme: ExternalProgramme,
        start: Date,
        microcycles: NonEmpty<CyclingMicrocycle>,
        weekdays: CyclingWeekdays,
    ) -> Result<Self, InvalidCyclingMesocycle> {
        for (at, microcycle) in microcycles.iter().enumerate() {
            for position in weekdays.positions() {
                if microcycle.ride(position).is_none() {
                    return Err(InvalidCyclingMesocycle::MissingRide {
                        microcycle: at + 1,
                        position,
                    });
                }
            }
        }
        ProvidedFrom::new(
            programme.clone(),
            microcycles
                .iter()
                .map(CyclingMicrocycle::published_ordinal)
                .collect(),
        )?;
        Ok(Self {
            programme,
            start,
            microcycles,
            weekdays,
        })
    }

    pub const fn programme(&self) -> &ExternalProgramme {
        &self.programme
    }

    /// Which microcycles of which published programme this is.
    ///
    /// **Derived rather than stored.** The numbers are on the microcycles and
    /// the programme is on the mesocycle, so a stored copy would be a second
    /// place for one fact and free to disagree with the rows it was built from.
    /// `new` refuses a mesocycle whose numbers do not make a selection, so this
    /// answers.
    #[must_use]
    pub fn provided_from(&self) -> Option<ProvidedFrom> {
        ProvidedFrom::new(
            self.programme.clone(),
            self.microcycles
                .iter()
                .map(CyclingMicrocycle::published_ordinal)
                .collect(),
        )
        .ok()
    }

    pub const fn start(&self) -> Date {
        self.start
    }

    pub const fn microcycles(&self) -> &NonEmpty<CyclingMicrocycle> {
        &self.microcycles
    }

    pub const fn weekdays(&self) -> &CyclingWeekdays {
        &self.weekdays
    }

    /// How many weeks it runs.
    #[must_use]
    pub const fn duration_weeks(&self) -> usize {
        self.microcycles.count()
    }

    /// One microcycle, counting from one as the programme itself does.
    #[must_use]
    pub fn microcycle(&self, number: usize) -> Option<&CyclingMicrocycle> {
        number
            .checked_sub(1)
            .and_then(|index| self.microcycles.iter().nth(index))
    }

    /// Which microcycle a date falls in, counting from one.
    ///
    /// `None` before the start or after the last microcycle. **Calendar weeks
    /// from the start date**, so a week the operator misses is a week of the
    /// programme all the same — the same rule the gym's calendar applies.
    #[must_use]
    pub fn microcycle_of(&self, date: Date) -> Option<usize> {
        if date < self.start {
            return None;
        }
        let days = date.since(self.start).ok()?.get_days();
        let number = usize::try_from(days / 7).ok()?.checked_add(1)?;
        (number <= self.duration_weeks()).then_some(number)
    }

    /// What is ridden on a date: which microcycle, which session, and the ride.
    ///
    /// `None` for a date this programme does not cover, or a weekday it does not
    /// ride.
    #[must_use]
    pub fn on(&self, date: Date) -> Option<(usize, SessionPosition, &PlannedRide)> {
        let number = self.microcycle_of(date)?;
        let position = self.weekdays.session_on(date)?;
        let ride = self.microcycle(number)?.ride(position)?;
        Some((number, position, ride))
    }

    /// The first date at or after `from` that this programme rides.
    ///
    /// Looks a week ahead and no further: the weekday map names weekdays, so if
    /// none of the next seven days rides, none ever will.
    #[must_use]
    pub fn next_riding_day(&self, from: Date) -> Option<Date> {
        let from = from.max(self.start);
        (0..7).find_map(|offset| {
            let date = from.checked_add(jiff::Span::new().days(offset)).ok()?;
            self.on(date).map(|_| date)
        })
    }
}
