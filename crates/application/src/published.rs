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
        SessionPosition, shape::partition,
    },
    planner::{Relative, SessionRole},
    provider::PublishedAt,
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

/// `microcycles` of the whole programme, as one mesocycle (#732).
///
/// **Chosen across the programme's own mesocycles**, where
/// [`mesocycles_of`] asks each separately: after a lost week starts it again,
/// the weeks left in the term are more than one of its halves holds. The
/// operator, 2026-11-09, on six weeks of Peak's eight with one trough rather
/// than two: what is chosen must split as one mesocycle, so the trough between
/// the halves goes and only the last remains.
#[must_use]
pub fn selection_of(read: &PublishedRead, microcycles: usize, sessions: usize) -> Option<Answer> {
    let whole = read.programme.microcycles();
    read.programme
        .answer(&whole, microcycles, sessions)
        .into_iter()
        .find(|answer| {
            let scores: Vec<f64> = answer
                .microcycles
                .iter()
                .map(|microcycle| {
                    read.programme
                        .profile(&[*microcycle], &answer.sessions)
                        .tss()
                })
                .collect();
            matches!(partition(&scores).as_slice(), [one] if *one == (0..scores.len()))
        })
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

/// The recovery week a published programme provides (#336), and which of its
/// sessions are ridden: `None` where it offers too few to make a week.
///
/// **Its first deload**: the last microcycle of the first of its mesocycles,
/// as [`partition`] finds them. The operator, 2026-11-12: *"Mesocycle
/// providers, like SBS and Peak need to be able to provide recovery weeks. For
/// Peak that's the deload in the middle of the published programme."* So Peak
/// and Base give µ4, and Build, which is one mesocycle, gives µ5.
///
/// **A ride measuring FTP is never taken**: recovery tests nothing, and the
/// next macrocycle opens with its own test. Of the rest, the first `sessions`
/// in published order, which in Peak µ4 and Build µ5 are two 45-minute
/// Endurance rides, so either may be the one dropped.
#[must_use]
pub fn recovery_of(read: &PublishedRead, sessions: usize) -> Option<(u32, Vec<u32>)> {
    let deload = *read.programme.mesocycles().first()?.last()?;
    let rides: Vec<u32> = read
        .programme
        .sessions()
        .into_iter()
        .filter(|session| {
            read.cells.contains_key(&(deload, *session)) && !measures(read, deload, &[*session])
        })
        .take(sessions)
        .collect();
    (rides.len() == sessions).then_some((deload, rides))
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
    built(
        CyclingProvenance::Provided(read.from.clone()),
        start,
        read,
        microcycles,
        sessions,
        riding_days,
    )
}

/// A recovery mesocycle of `weeks`, each the recovery week the programme
/// provides ([`recovery_of`]), recorded as that and not as microcycles of it
/// (#336).
///
/// # Errors
///
/// [`BuildError`] as [`build`], and where the programme provides no recovery
/// week.
pub fn recovery(
    start: Date,
    read: &PublishedRead,
    weeks: usize,
    sessions: usize,
    riding_days: &[Weekday],
) -> Result<CyclingMesocycle, BuildError> {
    let (deload, sessions) = recovery_of(read, sessions).ok_or_else(|| {
        BuildError::Source(SourceError::Malformed {
            detail: format!("{} provides no recovery week", read.from.name()),
        })
    })?;
    built(
        CyclingProvenance::Recovery(read.from.clone()),
        start,
        read,
        &vec![deload; weeks],
        &sessions,
        riding_days,
    )
}

/// The rides of `microcycles`, under `provenance`: each says where it was
/// published only where the mesocycle is microcycles of the programme.
fn built(
    provenance: CyclingProvenance,
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
            let planned = match provenance {
                CyclingProvenance::Provided(_) => PlannedRide::provided(
                    ride,
                    at,
                    PublishedAt::new(*number, session).map_err(|error| unbuildable(&error))?,
                    role,
                ),
                CyclingProvenance::Assembled | CyclingProvenance::Recovery(_) => {
                    PlannedRide::assembled(ride, at, role)
                }
            };
            rides.insert(position, planned);
        }
        weeks.push(CyclingMicrocycle::new(rides).map_err(|error| unbuildable(&error))?);
    }

    CyclingMesocycle::new(
        provenance,
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
