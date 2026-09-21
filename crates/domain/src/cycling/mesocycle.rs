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

use jiff::civil::Date;

use crate::{
    provider::{ExternalProgramme, InvalidProvision, ProvidedFrom, PublishedAt},
    schedule::{SessionRole, TrainingWeek},
    sequence::NonEmpty,
};

use super::{session::CyclingSession, venue::RideVenue};

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
/// a second session and says nothing about Wednesdays. **Nor does it decide
/// one** — until 2026-09-20 `CyclingWeekdays` mapped a position to a weekday,
/// which made this ordinal the scheduling fact it is not. What places a ride in
/// the week is [`PlannedRide::role`] meeting a training slot of the same role
/// (issue #63); this is the order the rides are authored in, and the way back
/// to what the publisher called them.
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

/// One ride, and where it is done.
///
/// **A session is one or more places** (decision 0033). The FTP warm-up and the
/// test itself are two classes and one session, so the venues are a sequence
/// and the [`CyclingSession`] beside them is the pair taken together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedRide {
    session: CyclingSession,
    at: NonEmpty<RideVenue>,
    published: Option<PublishedAt>,
    role: SessionRole,
}

impl PlannedRide {
    /// A ride taken from a published programme, at a stated coordinate in it.
    pub const fn provided(
        session: CyclingSession,
        at: NonEmpty<RideVenue>,
        published: PublishedAt,
        role: SessionRole,
    ) -> Self {
        Self {
            session,
            at,
            published: Some(published),
            role,
        }
    }

    /// A ride chosen here rather than taken from a programme.
    ///
    /// **The role is stated because nothing can derive it** (#180). A holding
    /// week's two rides are the same length, and the volume comparison admits
    /// equality, so which is the harder one is a fact about the *kind* of class
    /// rather than about its duration.
    pub const fn assembled(
        session: CyclingSession,
        at: NonEmpty<RideVenue>,
        role: SessionRole,
    ) -> Self {
        Self {
            session,
            at,
            published: None,
            role,
        }
    }

    pub const fn session(&self) -> &CyclingSession {
        &self.session
    }

    /// What this ride is against the microcycle's others: shorter and harder,
    /// or longer and easier.
    ///
    /// **Against the ones the operator rides, not the ones Peloton published.**
    /// A published microcycle offers three and he takes two; the two are roled
    /// against each other, and what the third would have been does not enter
    /// into it. The operator, 2026-09-20: *"the comparison is after the number
    /// of sessions have been decided"*.
    ///
    /// **It is this, and not [`Self::published`], that decides the
    /// weekday.** Peloton puts the FTP test second in its test microcycle,
    /// which would land it on the Sunday; it is the higher-intensity, shorter
    /// session, so it goes in the Wednesday slot. The operator, 2026-09-19:
    /// *"it does make more sense for the cycling test to keep the shorter /
    /// harder cycling slot on the Wednesday"*.
    pub const fn role(&self) -> SessionRole {
        self.role
    }

    /// Every place this ride is done, in the order they are ridden.
    pub const fn at(&self) -> &NonEmpty<RideVenue> {
        &self.at
    }

    /// Where in a published programme this ride came from.
    ///
    /// Kept so a re-authoring that wants the session the operator did *not*
    /// take knows which microcycle of which programme to ask for.
    ///
    /// `None` for a ride nobody published — a holding week's (#180), chosen out
    /// of the catalogue one class at a time. A mesocycle's rides agree about
    /// this: [`CyclingMesocycle::new`] refuses a mixture.
    pub const fn published(&self) -> Option<PublishedAt> {
        self.published
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidMicrocycle {
    #[error("a microcycle with no ride in it is a week off, not a microcycle")]
    NoRides,
    /// Some of the week's rides name a published microcycle and some do not.
    ///
    /// A week is taken from a published programme or it is not; there is no
    /// half of one. A mixture would leave [`CyclingMicrocycle::published_ordinal`]
    /// with two answers and no way to prefer either.
    #[error("some rides of this microcycle name a published week and some name none")]
    MixedProvenance,
    /// Two rides claiming to be from different published microcycles.
    ///
    /// The week is one week. Rides from µ2 and µ4 in one microcycle is a
    /// transcription error rather than an unusual programme, and nothing below
    /// could say which of them the week is.
    #[error("this microcycle's rides come from published weeks {first} and {second}")]
    SplitPublishedWeek { first: u32, second: u32 },
    /// Two rides the planner cannot tell apart.
    ///
    /// A role is what places a ride in the week (issue #63), so a microcycle
    /// holding two rides in the same role offers the planner no way to say
    /// which of them Wednesday gets. Roles are relative *within* the microcycle
    /// — if two rides really are alike, the week holds one session, not two.
    #[error("two rides in this microcycle are both {role}, and a week has one slot for each")]
    RepeatedRole { role: SessionRole },
}

/// One week of an authored programme: what is ridden, and in what order.
///
/// Keyed on this programme's own session numbering, so the first entry is the
/// first ride of the week whichever published session it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CyclingMicrocycle {
    rides: BTreeMap<SessionPosition, PlannedRide>,
}

impl CyclingMicrocycle {
    /// # Errors
    ///
    /// [`InvalidMicrocycle::NoRides`] if it prescribes nothing,
    /// [`InvalidMicrocycle::RepeatedRole`] if two of its rides are the same
    /// thing relative to each other, [`InvalidMicrocycle::MixedProvenance`] if
    /// only some of them name a published week, and
    /// [`InvalidMicrocycle::SplitPublishedWeek`] if they name two.
    pub fn new(rides: BTreeMap<SessionPosition, PlannedRide>) -> Result<Self, InvalidMicrocycle> {
        if rides.is_empty() {
            return Err(InvalidMicrocycle::NoRides);
        }
        for (at, (_, ride)) in rides.iter().enumerate() {
            if rides
                .values()
                .skip(at + 1)
                .any(|later| later.role() == ride.role())
            {
                return Err(InvalidMicrocycle::RepeatedRole { role: ride.role() });
            }
        }

        // The week's provenance is its rides', and they have to agree: a
        // microcycle *is* one week of one programme, or of none.
        let mut published: Option<u32> = None;
        for (index, ride) in rides.values().enumerate() {
            match (index, ride.published()) {
                (0, Some(at)) => published = Some(at.microcycle()),
                (0, None) => published = None,
                (_, Some(at)) => match published {
                    None => return Err(InvalidMicrocycle::MixedProvenance),
                    Some(first) if first != at.microcycle() => {
                        return Err(InvalidMicrocycle::SplitPublishedWeek {
                            first,
                            second: at.microcycle(),
                        });
                    }
                    Some(_) => {}
                },
                (_, None) => {
                    if published.is_some() {
                        return Err(InvalidMicrocycle::MixedProvenance);
                    }
                }
            }
        }

        Ok(Self { rides })
    }

    pub const fn rides(&self) -> &BTreeMap<SessionPosition, PlannedRide> {
        &self.rides
    }

    #[must_use]
    pub fn ride(&self, at: SessionPosition) -> Option<&PlannedRide> {
        self.rides.get(&at)
    }

    /// The ride this week runs in a given role, and where it sits in the week.
    ///
    /// **This is the join.** A training slot states a discipline and a role;
    /// this answers with the ride that fills it. At most one can match, which
    /// [`Self::new`] guarantees.
    #[must_use]
    pub fn for_role(&self, role: SessionRole) -> Option<(SessionPosition, &PlannedRide)> {
        self.rides
            .iter()
            .find(|(_, ride)| ride.role() == role)
            .map(|(position, ride)| (*position, ride))
    }

    /// Every role this week rides, in a stable order.
    ///
    /// **Sorted, not in session order.** Every microcycle rides the same two
    /// roles and the harder one always takes the same slot; what changes
    /// between weeks is which *published* session carries it. The operator's
    /// autumn cycling mesocycle ends in a test microcycle, and Peloton files
    /// the FTP test second where an ordinary week's shorter ride is first.
    /// Comparing in session order would refuse that as a change of roles when
    /// nothing about the week has changed.
    #[must_use]
    pub fn roles(&self) -> Vec<SessionRole> {
        let mut roles: Vec<SessionRole> = self.rides.values().map(PlannedRide::role).collect();
        roles.sort_unstable();
        roles
    }

    /// Which microcycle of the published programme this is, in that
    /// programme's own numbering.
    ///
    /// An answer of µ1-2-4-5 keeps the four numbers the programme uses, so the
    /// third microcycle here says 4. Which programme they are microcycles *of*
    /// is [`CyclingMesocycle::provenance`], said once rather than on every week.
    ///
    /// **Derived from the rides, not stored beside them.** It was a field until
    /// 2026-09-20, which made it a second place for a fact the rides already
    /// carry and left it free to disagree with them. [`Self::new`] refuses a
    /// week whose rides name two published microcycles or only some of them, so
    /// reading the first ride's answers for all of them.
    ///
    /// `None` for a week nobody published (#180).
    #[must_use]
    pub fn published_ordinal(&self) -> Option<u32> {
        self.rides
            .values()
            .next()
            .and_then(|ride| ride.published().map(PublishedAt::microcycle))
    }

    /// How many sessions the week holds. What "the second of two" counts
    /// against.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.rides.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidCyclingMesocycle {
    /// The weeks do not agree about what the week is.
    ///
    /// Every microcycle of a mesocycle runs the same roles, because the
    /// operator's week gives cycling the same slots every week of it. A
    /// mesocycle whose second week drops the longer ride would leave Sunday
    /// with nothing to put in it, and nothing here could say which week was
    /// the mistake.
    ///
    /// **The same roles, and not necessarily at the same positions.** A
    /// mesocycle ending in a test microcycle rides exactly the roles the other
    /// weeks do; Peloton just files the FTP test as the later session. That is
    /// the whole point of a role placing a session rather than a position
    /// doing it.
    #[error(
        "microcycle {microcycle} rides {rode}, and the first rides {expected} — \
         every week of a mesocycle rides the same roles"
    )]
    RolesDiffer {
        microcycle: usize,
        rode: String,
        expected: String,
    },
    /// The published numbers are not a selection: one is taken twice.
    ///
    /// **Not reachable through [`CyclingMicrocycle::new`]**, which refuses a
    /// zero on its own — this is the rule that spans the weeks rather than one
    /// of them.
    #[error(transparent)]
    NotASelection(#[from] InvalidProvision),
    /// The mesocycle and its weeks disagree about who published them.
    ///
    /// Not reachable through the parts: a week knows whether *its own* rides
    /// name a published microcycle, and this knows whether a programme was
    /// named, and only here can the two be compared.
    #[error("this mesocycle says it is {says}, and {weeks}")]
    ProvenanceDiffers {
        says: &'static str,
        weeks: &'static str,
    },
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

/// Where a mesocycle's weeks came from.
///
/// **A variant rather than a nullable programme**, which is how the gym says
/// the same thing: `Progression::Provided` carries its `ProvidedFrom` beside
/// its payload, and the derived kinds carry neither. An `Option<ExternalProgramme>`
/// would have made "no programme" a missing field rather than a kind of
/// mesocycle.
///
/// **Two kinds, and the second arrived with #180.** Until 2026-09-20 a cycling
/// mesocycle was always microcycles of a Peloton programme, taken whole. A
/// holding week is not: it is classes picked out of the catalogue because the
/// other discipline is repeating a week, and the published programme it would
/// otherwise be a microcycle of does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CyclingProvenance {
    /// Microcycles of a published programme, taken in that programme's order.
    Provided(ExternalProgramme),
    /// Assembled here, a class at a time.
    ///
    /// **It names no programme on purpose.** Calling Peloton the provider would
    /// be true of the classes and false of the week: nobody published this
    /// microcycle, and a re-authoring has nothing to ask for. What the classes
    /// are is recorded where it belongs — in the venues each ride names.
    Assembled,
}

impl CyclingProvenance {
    /// The published programme, where one published it.
    #[must_use]
    pub const fn programme(&self) -> Option<&ExternalProgramme> {
        match self {
            Self::Provided(programme) => Some(programme),
            Self::Assembled => None,
        }
    }
}

impl std::fmt::Display for CyclingProvenance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Provided(programme) => write!(f, "{programme}"),
            Self::Assembled => f.write_str("a holding microcycle"),
        }
    }
}

/// An authored cycling mesocycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CyclingMesocycle {
    provenance: CyclingProvenance,
    /// The Monday microcycle one begins on.
    start: Date,
    microcycles: NonEmpty<CyclingMicrocycle>,
    /// The Mondays of weeks the concurrent microcycle was lost in, ascending.
    ///
    /// **Derived when the plan is read, never authored or stored** (#177). A
    /// week whose essential sessions were not performed is run again, so the
    /// microcycle it held moves to the week after and everything behind it
    /// moves with it — which is what a gym calendar does with a week it skips,
    /// and this is the same idea for a mesocycle with no calendar of its own.
    lost: Vec<Date>,
}

impl CyclingMesocycle {
    /// # Errors
    ///
    /// [`InvalidCyclingMesocycle::RolesDiffer`] where the microcycles do not
    /// all ride the same roles. That is the one thing the parts cannot
    /// guarantee between them, and leaving it unchecked would author a
    /// programme with a Sunday nothing answers for.
    ///
    /// [`InvalidCyclingMesocycle::NotASelection`] where two microcycles claim
    /// the same published week. A selection takes each week once, and µ1-2-2-4
    /// is not one anybody made.
    ///
    /// [`InvalidCyclingMesocycle::ProvenanceDiffers`] where the weeks and the
    /// mesocycle disagree about whether anybody published this.
    pub fn new(
        provenance: CyclingProvenance,
        start: Date,
        microcycles: NonEmpty<CyclingMicrocycle>,
    ) -> Result<Self, InvalidCyclingMesocycle> {
        let expected = microcycles.first().roles();
        for (at, microcycle) in microcycles.iter().enumerate().skip(1) {
            let rode = microcycle.roles();
            if rode != expected {
                return Err(InvalidCyclingMesocycle::RolesDiffer {
                    microcycle: at + 1,
                    rode: describe(&rode),
                    expected: describe(&expected),
                });
            }
        }

        // The weeks say what they are and so does this; the two must agree.
        // A `Provided` mesocycle of unpublished weeks would leave
        // `provided_from` with a programme and no numbers, and an `Assembled`
        // one of published weeks would throw away the way back to them.
        let ordinals: Vec<Option<u32>> = microcycles
            .iter()
            .map(CyclingMicrocycle::published_ordinal)
            .collect();
        match &provenance {
            CyclingProvenance::Provided(programme) => {
                let published: Option<Vec<u32>> = ordinals.iter().copied().collect();
                let published = published.ok_or(InvalidCyclingMesocycle::ProvenanceDiffers {
                    says: "taken from a published programme",
                    weeks: "at least one week names none",
                })?;
                ProvidedFrom::new(programme.clone(), published)?;
            }
            CyclingProvenance::Assembled => {
                if ordinals.iter().any(Option::is_some) {
                    return Err(InvalidCyclingMesocycle::ProvenanceDiffers {
                        says: "assembled here",
                        weeks: "at least one week names a published microcycle",
                    });
                }
            }
        }

        Ok(Self {
            provenance,
            start,
            microcycles,
            lost: Vec::new(),
        })
    }

    pub const fn provenance(&self) -> &CyclingProvenance {
        &self.provenance
    }

    /// The published programme, where one published it.
    #[must_use]
    pub const fn programme(&self) -> Option<&ExternalProgramme> {
        self.provenance.programme()
    }

    /// Which microcycles of which published programme this is.
    ///
    /// **Derived rather than stored.** The numbers are on the rides and the
    /// programme is here, so a stored copy would be a second place for one fact
    /// and free to disagree with the rows it was built from. `new` refuses a
    /// mesocycle whose numbers do not make a selection, so this answers.
    ///
    /// `None` for a mesocycle nobody published (#180).
    #[must_use]
    pub fn provided_from(&self) -> Option<ProvidedFrom> {
        let programme = self.provenance.programme()?;
        let published: Option<Vec<u32>> = self
            .microcycles
            .iter()
            .map(CyclingMicrocycle::published_ordinal)
            .collect();
        ProvidedFrom::new(programme.clone(), published?).ok()
    }

    pub const fn start(&self) -> Date {
        self.start
    }

    /// The same mesocycle, beginning on another Monday.
    ///
    /// **What a holding week costs the weeks after it** (#180). Inserting one
    /// occupies a week nothing else can, and a plan refuses two mesocycles over
    /// one day — so everything after it moves back by the same amount. Nothing
    /// about what is ridden changes, which is why this copies rather than
    /// re-validates: the parts that `new` checks are unchanged by the date.
    #[must_use]
    pub fn starting_on(&self, start: Date) -> Self {
        Self {
            provenance: self.provenance.clone(),
            start,
            microcycles: self.microcycles.clone(),
            // A mesocycle that has moved begins afresh: a week it lost was lost
            // where it used to be, and that week is not in the moved span.
            lost: Vec::new(),
        }
    }

    /// The same mesocycle, with the week beginning `monday` lost (#177).
    ///
    /// The microcycle that week held is ridden the week after, and the span
    /// grows by one. A Monday outside the span, or already lost, changes
    /// nothing.
    #[must_use]
    pub fn losing(&self, monday: Date) -> Self {
        let mut lost = self.lost.clone();
        let inside = monday >= self.start && monday < self.end();
        if inside && !lost.contains(&monday) {
            lost.push(monday);
            lost.sort_unstable();
        }
        Self {
            lost,
            ..self.clone()
        }
    }

    /// The Mondays of the weeks this mesocycle lost.
    #[must_use]
    pub fn lost(&self) -> &[Date] {
        &self.lost
    }

    /// Calendar weeks occupied: every microcycle, and every week lost.
    #[must_use]
    pub const fn calendar_weeks(&self) -> usize {
        self.duration_weeks().saturating_add(self.lost.len())
    }

    /// The day after the last one this mesocycle occupies.
    fn end(&self) -> Date {
        let weeks = i64::try_from(self.calendar_weeks()).unwrap_or(i64::MAX);
        self.start
            .checked_add(jiff::Span::new().weeks(weeks))
            .unwrap_or(self.start)
    }

    pub const fn microcycles(&self) -> &NonEmpty<CyclingMicrocycle> {
        &self.microcycles
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
    ///
    /// **A lost week answers `None`**, and every week after it one microcycle
    /// earlier than its calendar position would say (#177): the microcycle it
    /// held was not completed, so it is the one run next.
    #[must_use]
    pub fn microcycle_of(&self, date: Date) -> Option<usize> {
        if date < self.start {
            return None;
        }
        let days = date.since(self.start).ok()?.get_days();
        let week = usize::try_from(days / 7).ok()?;
        let monday = self
            .start
            .checked_add(jiff::Span::new().weeks(i64::try_from(week).ok()?))
            .ok()?;
        if self.lost.contains(&monday) {
            return None;
        }
        let before = self.lost.iter().filter(|lost| **lost < monday).count();
        let number = week.checked_sub(before)?.checked_add(1)?;
        (number <= self.duration_weeks()).then_some(number)
    }

    /// What is ridden on a date: which microcycle, which session, and the ride.
    ///
    /// **The week decides, not the programme.** `week` is cycling's slots as
    /// the diary gives them; the slot on this date names a role, and the ride
    /// answering to that role is what is ridden. Until 2026-09-20 the
    /// programme carried its own weekday map and this took no argument, which
    /// is the duplication issue #63 removes.
    ///
    /// `None` for a date this programme does not cover, a weekday cycling does
    /// not ride, or a role the microcycle has no ride for.
    #[must_use]
    pub fn on(
        &self,
        date: Date,
        week: &TrainingWeek,
    ) -> Option<(usize, SessionPosition, &PlannedRide)> {
        let number = self.microcycle_of(date)?;
        let role = week.role_on(date.weekday())?;
        let (position, ride) = self.microcycle(number)?.for_role(role)?;
        Some((number, position, ride))
    }

    /// The first date at or after `from` that this programme rides.
    ///
    /// Looks a week ahead and no further: the week names weekdays, so if none
    /// of the next seven days rides, none ever will.
    #[must_use]
    pub fn next_riding_day(&self, from: Date, week: &TrainingWeek) -> Option<Date> {
        let from = from.max(self.start);
        (0..7).find_map(|offset| {
            let date = from.checked_add(jiff::Span::new().days(offset)).ok()?;
            self.on(date, week).map(|_| date)
        })
    }
}

/// A list of roles in a sentence, for a refusal that names both sides.
fn describe(roles: &[SessionRole]) -> String {
    let named: Vec<String> = roles.iter().map(ToString::to_string).collect();
    match named.split_last() {
        None => "nothing".to_owned(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}
