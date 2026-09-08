//! Reading a published programme out of Peloton: as a shape the planner can
//! ask, and as the sessions the store can hold.
//!
//! **The skeleton is given and the content is fetched** (decision 0033).
//! [`fetch`] does the fetching, once, and the two things anyone wants from it
//! are derived from what it returns rather than from a second round of
//! requests: [`programme`] is the grid of zone profiles a provider is asked for,
//! and [`session`] is one cell as a ride that can be authored.
//!
//! **The grid holds no identifiers.** A [`PublishedProgramme`] is zone profiles and
//! nothing of Peloton's, because a provider answers with a shape and the domain
//! holds no vendor's identifiers (§ II.3). What names a class travels with the
//! ride instead, as a [`RideVenue`] the domain does not interpret.
//!
//! **It fails loudly.** Every class is fetched; one that will not read stops the
//! whole programme rather than yielding a shape with a hole in it, because a
//! mesocycle scored from fourteen of sixteen classes looks exactly like one
//! scored from all of them.

use std::collections::BTreeMap;

use application::SourceError;
use domain::{
    cycling::{CyclingSession, Interval, PublishedProgramme, Ride, RideVenue, ZoneProfile},
    measure::PositiveDuration,
    sequence::NonEmpty,
};

use super::{
    class::{ClassSession, PelotonClasses},
    skeleton::Placement,
};

/// What a skeleton's classes are, once fetched: the classes at each
/// `(microcycle, session)`, in the order they are placed.
pub type Fetched = BTreeMap<(u32, u32), Vec<ClassSession>>;

/// Fetch every class a skeleton places.
///
/// **A session is one or more classes** (0033): the FTP warm-up and test pair
/// share a cell, so classes accumulate rather than replace.
///
/// # Errors
///
/// [`SourceError`] from the first class that will not fetch or will not read.
pub async fn fetch(
    classes: &PelotonClasses,
    placements: &[Placement],
) -> Result<Fetched, SourceError> {
    let mut cells: Fetched = BTreeMap::new();
    for placement in placements {
        let class = classes.class(placement.class_id).await?;
        cells
            .entry((
                u32::from(placement.microcycle),
                u32::from(placement.session),
            ))
            .or_default()
            .push(class);
    }
    Ok(cells)
}

/// What the programme trains, as a grid of zone profiles.
#[must_use]
pub fn programme(fetched: &Fetched) -> PublishedProgramme {
    PublishedProgramme::new(fetched.iter().map(|(at, classes)| {
        (
            *at,
            ZoneProfile::of(classes.iter().filter_map(|class| class.ride.as_ref())),
        )
    }))
}

/// One cell as a ride: the session it prescribes, and where it is done.
///
/// **The classes are joined rather than listed.** A warm-up class and the test
/// that follows it are one session of two places, so the warm-ups sum, the
/// cool-downs sum, and the working parts run on from one another.
///
/// # Errors
///
/// [`SourceError::Malformed`] where the cell holds no class, no working part at
/// all, or both an effort and a zone plan — a session cannot both measure the
/// number and prescribe shares of it.
pub fn session(
    at: (u32, u32),
    classes: &[ClassSession],
) -> Result<(CyclingSession, NonEmpty<RideVenue>), SourceError> {
    let malformed = |detail: String| SourceError::Malformed { detail };
    let (microcycle, position) = at;
    let where_it_is = format!("microcycle {microcycle} session {position}");

    let warm_up: u64 = classes.iter().map(|class| class.warm_up_seconds).sum();
    let cool_down: u64 = classes.iter().map(|class| class.cool_down_seconds).sum();

    let mut intervals: Vec<Interval> = Vec::new();
    let mut effort: Option<PositiveDuration> = None;
    for class in classes {
        match &class.ride {
            // All warm-up and no ride: the FTP warm-up class is exactly this,
            // and it contributes its minutes above and nothing here.
            None => {}
            Some(Ride::Intervals(runs)) => intervals.extend(runs.iter().copied()),
            Some(Ride::Effort(duration)) if effort.is_none() => effort = Some(*duration),
            Some(Ride::Effort(_)) => {
                return Err(malformed(format!(
                    "{where_it_is} holds two efforts, and a session measures once"
                )));
            }
        }
    }

    let ride = match (effort, intervals.is_empty()) {
        (Some(duration), true) => Ride::Effort(duration),
        (None, false) => Ride::Intervals(
            NonEmpty::new(intervals)
                .map_err(|error| malformed(format!("{where_it_is}: {error}")))?,
        ),
        (Some(_), false) => {
            return Err(malformed(format!(
                "{where_it_is} both measures FTP and prescribes shares of it"
            )));
        }
        (None, true) => {
            return Err(malformed(format!("{where_it_is} instructs no riding")));
        }
    };

    let warm_up = PositiveDuration::from_seconds(warm_up)
        .map_err(|error| malformed(format!("{where_it_is} has no warm-up: {error}")))?;
    // Absent rather than zero: the FTP test ships with no cool-down section at
    // all, and a zero-length one would be inventing a section it does not have.
    let cool_down = (cool_down > 0)
        .then(|| PositiveDuration::from_seconds(cool_down))
        .transpose()
        .map_err(|error| malformed(format!("{where_it_is}: {error}")))?;

    let venues = classes
        .iter()
        .map(|class| {
            RideVenue::new(&class.id, &class.title)
                .map_err(|error| malformed(format!("{where_it_is}: {error}")))
        })
        .collect::<Result<Vec<_>, SourceError>>()?;
    let venues =
        NonEmpty::new(venues).map_err(|_| malformed(format!("{where_it_is} names no class")))?;

    Ok((CyclingSession::new(warm_up, ride, cool_down), venues))
}
