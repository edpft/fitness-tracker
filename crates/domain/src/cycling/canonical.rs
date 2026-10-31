//! The cycling session that happened, however many sources recorded it (#408).
//!
//! The canonical layer's entity for cycling (§ II.4), and the gym's
//! [`crate::gym::CanonicalGymSession`]'s sibling. **The unit is the session**
//! (operator, 2026-10-12): an FTP test is one canonical session of three Bike+
//! rides, as it is one normalised session, and never three entries `next`
//! would have to group back together.
//!
//! **One source today.** Every one of the operator's rides is Peloton's
//! account. Garmin holds 218 of them too, but each is an upload of Peloton's
//! own ride rather than a second recording, so there is nothing to merge
//! (operator, 2026-10-12). Each field still names the normalised session it was
//! taken from, so a second account — Wahoo (#245), or a ride the watch records
//! — is a merge added here rather than a reshaped layer.

use std::fmt;

use crate::canonical::{Attributed, NormalisedSessionId, Reach};
use crate::device::Sighting;
use crate::measure::{Duration, HeartRateSeries, Metres};
use crate::normalised::StartedAt;
use crate::sequence::NonEmpty;

use super::ride::RideSample;
use super::venue::RideVenue;
use super::zone::Watts;

/// One ride on a Peloton Bike+, field by field.
///
/// **Still a Bike+ ride**, for the reason [`BikePlusRide`] is: the instrument
/// is part of what its power and speed mean (§ 6), so a canonical ride does not
/// abstract over it either. What it drops is what was only true of the What it drops is what was only true of the
/// transport — the landing records, the source's id, the endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalBikePlusRide {
    started_at: Attributed<StartedAt>,
    at: Attributed<RideVenue>,
    duration: Attributed<Duration>,
    distance: Attributed<Metres>,
    average_power: Attributed<Watts>,
    samples: Attributed<NonEmpty<RideSample>>,
    heart_rate: Option<Attributed<HeartRateSeries>>,
    heart_rate_declared_missing: Option<Attributed<Duration>>,
    heart_rate_device: Option<Attributed<Sighting>>,
}

/// What a canonical ride is built from, so the constructor is not eight
/// positional arguments.
pub struct CanonicalRideRecord {
    pub started_at: Attributed<StartedAt>,
    pub at: Attributed<RideVenue>,
    pub duration: Attributed<Duration>,
    pub distance: Attributed<Metres>,
    pub average_power: Attributed<Watts>,
    pub samples: Attributed<NonEmpty<RideSample>>,
    pub heart_rate: Option<Attributed<HeartRateSeries>>,
    pub heart_rate_declared_missing: Option<Attributed<Duration>>,
    pub heart_rate_device: Option<Attributed<Sighting>>,
}

impl CanonicalBikePlusRide {
    pub fn new(record: CanonicalRideRecord) -> Self {
        Self {
            started_at: record.started_at,
            at: record.at,
            duration: record.duration,
            distance: record.distance,
            average_power: record.average_power,
            samples: record.samples,
            heart_rate: record.heart_rate,
            heart_rate_declared_missing: record.heart_rate_declared_missing,
            heart_rate_device: record.heart_rate_device,
        }
    }

    pub const fn started_at(&self) -> &Attributed<StartedAt> {
        &self.started_at
    }

    /// The class it was ridden to.
    pub const fn at(&self) -> &Attributed<RideVenue> {
        &self.at
    }

    pub const fn duration(&self) -> &Attributed<Duration> {
        &self.duration
    }

    pub const fn distance(&self) -> &Attributed<Metres> {
        &self.distance
    }

    /// What the source stated the ride averaged — see
    /// [`super::BikePlusRide::average_power`].
    pub const fn average_power(&self) -> &Attributed<Watts> {
        &self.average_power
    }

    pub const fn samples(&self) -> &Attributed<NonEmpty<RideSample>> {
        &self.samples
    }

    pub const fn heart_rate(&self) -> Option<&Attributed<HeartRateSeries>> {
        self.heart_rate.as_ref()
    }

    /// How many seconds the source says it did not measure — see
    /// [`super::RideRecord::heart_rate_declared_missing`].
    pub const fn heart_rate_declared_missing(&self) -> Option<&Attributed<Duration>> {
        self.heart_rate_declared_missing.as_ref()
    }

    /// The monitor it was ridden with — see
    /// [`super::RideRecord::heart_rate_device`].
    pub const fn heart_rate_device(&self) -> Option<&Attributed<Sighting>> {
        self.heart_rate_device.as_ref()
    }

    /// Every normalised session a field of it names, each once.
    fn normalised_sessions(&self) -> Vec<NormalisedSessionId> {
        let mut ids = vec![
            self.started_at.normalised_session(),
            self.at.normalised_session(),
            self.duration.normalised_session(),
            self.distance.normalised_session(),
            self.average_power.normalised_session(),
            self.samples.normalised_session(),
        ];
        ids.extend(self.heart_rate.as_ref().map(Attributed::normalised_session));
        ids.extend(
            self.heart_rate_declared_missing
                .as_ref()
                .map(Attributed::normalised_session),
        );
        ids.extend(
            self.heart_rate_device
                .as_ref()
                .map(Attributed::normalised_session),
        );
        ids
    }
}

/// A cycling session, however many sources recorded it.
///
/// **The variants are [`PerformedSession`]'s**, for its reason: a test without
/// a warm-up is unrepresentable rather than rejected (§ 24).
#[expect(
    clippy::large_enum_variant,
    reason = "a test holds one more ride than a ride does; boxing it would put an allocation between a session and its parts, as in `PerformedSession`"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalCyclingSession {
    Ride {
        main: CanonicalBikePlusRide,
        cool_down: Option<CanonicalBikePlusRide>,
    },
    Test {
        warm_up: CanonicalBikePlusRide,
        effort: CanonicalBikePlusRide,
        cool_down: Option<CanonicalBikePlusRide>,
    },
}

impl CanonicalCyclingSession {
    /// Every ride, in the order it was ridden.
    pub fn rides(&self) -> Vec<&CanonicalBikePlusRide> {
        match self {
            Self::Ride { main, cool_down } => {
                let mut rides = vec![main];
                rides.extend(cool_down.as_ref());
                rides
            }
            Self::Test {
                warm_up,
                effort,
                cool_down,
            } => {
                let mut rides = vec![warm_up, effort];
                rides.extend(cool_down.as_ref());
                rides
            }
        }
    }

    /// The ride the session is about: the main ride, or the test's effort.
    pub const fn working_ride(&self) -> &CanonicalBikePlusRide {
        match self {
            Self::Ride { main, .. } => main,
            Self::Test { effort, .. } => effort,
        }
    }

    /// When it started, which is when its first ride did.
    pub const fn started_at(&self) -> &StartedAt {
        match self {
            Self::Ride { main, .. } => main.started_at.value(),
            Self::Test { warm_up, .. } => warm_up.started_at.value(),
        }
    }

    /// Every normalised session it stands on, each once, in the order its
    /// rides name them.
    pub fn normalised_sessions(&self) -> Vec<NormalisedSessionId> {
        let mut ids: Vec<NormalisedSessionId> = Vec::new();
        for id in self
            .rides()
            .into_iter()
            .flat_map(CanonicalBikePlusRide::normalised_sessions)
        {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        ids
    }

    /// The stable key for which variant this is. Persisted.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Ride { .. } => "ride",
            Self::Test { .. } => "test",
        }
    }
}

impl fmt::Display for CanonicalCyclingSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {}, {} rides",
            self.started_at(),
            self.kind(),
            self.rides().len()
        )
    }
}

/// The canonical cycling layer the normalised one gives, oldest first.
///
/// `accounts` are each normalised session in the canonical shape, every field
/// attributed to itself, as [`crate::gym::NormalisedGymSession`] is for the
/// gym. **One canonical session per account**, because the only account of a
/// ride is Peloton's (see the module). Deterministic (§ 9): ordered by start,
/// ties broken by the normalised session a session's first field names.
pub fn canonical_sessions(
    mut accounts: Vec<CanonicalCyclingSession>,
) -> Vec<CanonicalCyclingSession> {
    accounts.sort_by_key(|session| {
        (
            session.started_at().instant(),
            session.normalised_sessions().first().copied(),
        )
    });
    accounts
}

/// How many days either side of its own day a ride's account can change an
/// entry on (#610): none, since each account is its own session.
pub const REACHES: i64 = 0;

/// The canonical cycling sessions on the days `reach` rebuilds, from whichever
/// of `accounts` fall on the days it reads (#610). Accounts outside those days
/// are ignored.
pub fn canonical_sessions_within(
    accounts: Vec<CanonicalCyclingSession>,
    reach: &Reach,
) -> Vec<CanonicalCyclingSession> {
    let on = |session: &CanonicalCyclingSession| session.started_at().wall_clock().date();
    let read = accounts
        .into_iter()
        .filter(|account| reach.read().contains(&on(account)))
        .collect();
    canonical_sessions(read)
        .into_iter()
        .filter(|session| reach.rebuilt().contains(&on(session)))
        .collect()
}
