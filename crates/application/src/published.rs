//! Turning a published cycling programme into mesocycles the plan can hold.
//!
//! **Shared by the `plan` wizard and by committing the next mesocycle** (#397).
//! Both read a programme through [`PublishedProgrammes`](crate::PublishedProgrammes)
//! and author part of it; this is the part that decides which part, and what
//! each ride's role is, so the two cannot disagree about it.

use std::collections::BTreeMap;

use domain::{
    cycling::{
        Answer, CyclingMesocycle, CyclingMicrocycle, CyclingProvenance, PlannedRide,
        SessionPosition,
    },
    provider::PublishedAt,
    schedule::{Relative, SessionRole},
    sequence::NonEmpty,
};
use jiff::civil::{Date, Weekday};

use crate::{PublishedRead, SourceError};

/// The harder session of a week: the shorter one, and the one a test replaces.
pub const HARDER: SessionRole = SessionRole::new(Relative::Higher, Relative::Lower);
/// The easier one.
pub const EASIER: SessionRole = SessionRole::new(Relative::Lower, Relative::Higher);

/// Why a published programme would not make a mesocycle.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BuildError {
    /// The week has fewer riding days than the answer has rides.
    #[error(
        "the schedule gives cycling {days} day(s) a week and this answer rides {rides}; \
         record the week the block is trained in first"
    )]
    TooFewDays { days: usize, rides: usize },
    #[error(transparent)]
    Source(#[from] SourceError),
    /// What was built is not a mesocycle the domain accepts.
    #[error("{detail}")]
    Unbuildable { detail: String },
}

fn unbuildable(error: &impl std::fmt::Display) -> BuildError {
    BuildError::Unbuildable {
        detail: error.to_string(),
    }
}

/// One mesocycle of a published programme, and what it answers when asked
/// for a smaller shape.
#[derive(Debug, Clone, PartialEq)]
pub struct Offered {
    /// Which of the programme's mesocycles, from zero.
    pub index: usize,
    /// How many microcycles it publishes.
    pub own: usize,
    pub answer: Option<Answer>,
}

/// Split a programme and ask each mesocycle for the shape wanted.
#[must_use]
pub fn mesocycles_of(read: &PublishedRead, microcycles: usize, sessions: usize) -> Vec<Offered> {
    read.programme
        .mesocycles()
        .into_iter()
        .enumerate()
        .map(|(index, mesocycle)| Offered {
            index,
            own: mesocycle.len(),
            answer: read
                .programme
                .answer(&mesocycle, microcycles, sessions)
                .into_iter()
                .next(),
        })
        .collect()
}

/// Whether any session ridden in this microcycle measures FTP.
///
/// **The class says so, not the programme.** Peloton marks the test rides
/// themselves, which is what makes this answerable for a microcycle chosen out
/// of the middle of a programme — Build's µ5 is a test week whether it is ridden
/// as Build's fifth or as our fourth.
#[must_use]
pub fn measures(read: &PublishedRead, microcycle: u32, sessions: &[u32]) -> bool {
    sessions.iter().any(|session| {
        read.cells
            .get(&(microcycle, *session))
            .is_some_and(|cell| cell.measures_ftp)
    })
}

/// Which of the test programme's sessions are ridden.
///
/// **The FTP test is taken, and the rest of the week fills up around it.** A
/// test week that dropped the session measuring FTP would be a week of ordinary
/// riding wearing the name.
///
/// The microcycle is the programme's own first and only one, so there is nothing
/// to choose there — which is the point of it being a programme rather than a
/// microcycle of Build.
#[must_use]
pub fn test_microcycle(read: &PublishedRead, sessions: usize) -> Option<Vec<u32>> {
    let number = read.programme.microcycles().first().copied()?;

    let mut taken: Vec<u32> = read
        .programme
        .sessions()
        .into_iter()
        .filter(|session| measures(read, number, &[*session]))
        .collect();
    for session in read.programme.sessions() {
        if taken.len() >= sessions {
            break;
        }
        if !taken.contains(&session) {
            taken.push(session);
        }
    }
    taken.sort_unstable();
    Some(taken)
}

/// Turn an answer into a mesocycle the plan can hold.
///
/// **Both axes are renumbered from one and both keep their provenance.** An
/// answer of µ1-2-4-5 by sessions 1+3 authors four microcycles of two rides, and
/// each says which published microcycle and which published session it came
/// from. The operator rides a first and a second session in the week; that they
/// are the published first and third is a fact about where they were taken from,
/// not what to call them.
///
/// **A ride's role is read off its length, not off its published order.** The
/// shorter of the week's rides is the higher-intensity one — that is what the
/// pairing means (issue #177) — and it is what places it in the week: the
/// harder, shorter slot rather than the longer one. Peloton puts the FTP test
/// second in its test microcycle, which by published order would land it on
/// the Sunday; it is the shortest ride of that week, so it goes to the
/// Wednesday. The operator, 2026-09-19: *"it does make more sense for the
/// cycling test to keep the shorter / harder cycling slot on the Wednesday"*.
///
/// **A tie goes to the earlier session.** Two rides of exactly equal length
/// satisfy the rule whichever way round the roles go (the operator,
/// 2026-09-20), so the position break picks one and both are valid.
///
/// # Errors
///
/// [`BuildError`] if the week has too few riding days, if a cell asked for was
/// not read or does not make a ride, or if the result is not a mesocycle.
pub fn build(
    start: Date,
    read: &PublishedRead,
    microcycles: &[u32],
    sessions: &[u32],
    riding_days: &[Weekday],
) -> Result<CyclingMesocycle, BuildError> {
    if riding_days.len() < sessions.len() {
        return Err(BuildError::TooFewDays {
            days: riding_days.len(),
            rides: sessions.len(),
        });
    }

    // This programme's own numbering: the first ride of the week is session one
    // whichever published session it was taken from.
    let positions = (1..=sessions.len())
        .map(|ordinal| {
            u8::try_from(ordinal)
                .ok()
                .and_then(|number| SessionPosition::new(number).ok())
                .ok_or_else(|| unbuildable(&"more rides than a week can hold"))
        })
        .collect::<Result<Vec<_>, BuildError>>()?;

    let mut weeks = Vec::with_capacity(microcycles.len());
    for number in microcycles {
        let mut read_rides = Vec::with_capacity(positions.len());
        for (position, session) in positions.iter().zip(sessions) {
            let cell = read.cells.get(&(*number, *session)).ok_or_else(|| {
                BuildError::Source(SourceError::Malformed {
                    detail: format!(
                        "nothing was read for {} µ{number} session {session}",
                        read.from.name()
                    ),
                })
            })?;
            let (ride, at) = cell.ride.clone()?;
            read_rides.push((*position, ride, at, *session));
        }

        let shortest = read_rides
            .iter()
            .min_by_key(|(position, ride, _, _)| (ride.total(), *position))
            .map(|(position, _, _, _)| *position);

        let mut rides = BTreeMap::new();
        for (position, ride, at, session) in read_rides {
            let role = if Some(position) == shortest {
                HARDER
            } else {
                EASIER
            };
            let published =
                PublishedAt::new(*number, session).map_err(|error| unbuildable(&error))?;
            rides.insert(position, PlannedRide::provided(ride, at, published, role));
        }
        weeks.push(CyclingMicrocycle::new(rides).map_err(|error| unbuildable(&error))?);
    }

    CyclingMesocycle::new(
        CyclingProvenance::Provided(read.from.clone()),
        start,
        NonEmpty::new(weeks).map_err(|error| unbuildable(&error))?,
    )
    .map_err(|error| unbuildable(&error))
}

/// A mesocycle whose last microcycle's harder ride is replaced by the FTP test.
///
/// Our Base 2 (the operator, 2026-09-25): published Base has no test of its
/// own, and Build opens from one.
///
/// **The replaced ride keeps its place in Base**: its published coordinate is
/// the session it stands in for, which is the way back to what was not ridden.
///
/// # Errors
///
/// [`BuildError`] if the test programme holds no FTP test, or the mesocycle has
/// no harder ride in its last week to replace.
pub fn ending_in_a_test(
    mesocycle: &CyclingMesocycle,
    testing: &PublishedRead,
) -> Result<CyclingMesocycle, BuildError> {
    let missing = |detail: &str| {
        BuildError::Source(SourceError::Malformed {
            detail: detail.to_owned(),
        })
    };
    let test_session = testing
        .programme
        .sessions()
        .into_iter()
        .find(|session| measures(testing, 1, &[*session]))
        .ok_or_else(|| missing("the Power Zone test holds no FTP test"))?;
    let (ride, at) = testing
        .cells
        .get(&(1, test_session))
        .ok_or_else(|| missing("the FTP test was not read"))?
        .ride
        .clone()?;

    let count = mesocycle.duration_weeks();
    let mut weeks = Vec::with_capacity(count);
    for (index, week) in mesocycle.microcycles().iter().enumerate() {
        if index.saturating_add(1) < count {
            weeks.push(week.clone());
            continue;
        }
        let mut rides = week.rides().clone();
        let (position, replaced) = week
            .for_role(HARDER)
            .map(|(position, ride)| (position, ride.clone()))
            .ok_or_else(|| missing("Base's last week has no harder ride to replace"))?;
        let published = replaced
            .published()
            .ok_or_else(|| missing("Base's last week names no published session"))?;
        rides.insert(
            position,
            PlannedRide::provided(ride.clone(), at.clone(), published, HARDER),
        );
        weeks.push(CyclingMicrocycle::new(rides).map_err(|error| unbuildable(&error))?);
    }

    CyclingMesocycle::new(
        mesocycle.provenance().clone(),
        mesocycle.start(),
        NonEmpty::new(weeks).map_err(|error| unbuildable(&error))?,
    )
    .map_err(|error| unbuildable(&error))
}
