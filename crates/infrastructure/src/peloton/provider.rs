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

use application::{
    PublishedCell, PublishedCycling, PublishedProgrammes, PublishedRead, SourceError,
};
use domain::{
    cycling::{CyclingSession, Interval, PublishedProgramme, Ride, RideVenue, ZoneProfile},
    measure::PositiveDuration,
    provider::{ExternalProgramme, ProgrammeName, Provider},
    sequence::NonEmpty,
};

use super::{
    catalogue::ClassCatalogue,
    class::ClassSession,
    skeleton::{
        BOOST_YOUR_BASE, BUILD_YOUR_POWER_ZONES, POWER_ZONE_TEST, Placement, peak_your_power_zones,
        power_zone_test,
    },
};

/// What a skeleton's classes are, once fetched: the classes at each
/// `(microcycle, session)`, in the order they are placed.
pub type Fetched = BTreeMap<(u32, u32), Vec<ClassSession>>;

/// Read every class a skeleton places.
///
/// **From the catalogue, since #246.** It was sixty-five requests per
/// authoring, and the refresh reads the placed classes before any other, so a
/// store that has been refreshed once answers all of them locally.
///
/// **A session is one or more classes** (0033): the FTP warm-up and test pair
/// share a cell, so classes accumulate rather than replace.
///
/// # Errors
///
/// [`SourceError`] from the first class that will not fetch or will not read.
pub async fn fetch(
    classes: ClassCatalogue<'_>,
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

/// Who publishes the cycling programmes this build reads.
const PELOTON: &str = "Peloton";

/// Peloton's names for the published programmes.
const BASE: &str = "Boost Your Base";
const BUILD: &str = "Build Your Power Zones";
const PEAK: &str = "Peak Your Power Zones";

/// Peloton, as the publisher of the cycling programmes a plan is built from.
///
/// **From the local catalogue** ([`ClassCatalogue`]), so a store that has been
/// refreshed once reads every programme without contacting Peloton.
#[derive(Debug, Clone, Copy)]
pub struct PelotonProgrammes<'a> {
    classes: ClassCatalogue<'a>,
}

impl<'a> PelotonProgrammes<'a> {
    pub const fn new(classes: ClassCatalogue<'a>) -> Self {
        Self { classes }
    }
}

/// Peloton's name for a programme, and where its classes are placed.
fn skeleton(which: PublishedCycling) -> (&'static str, Vec<Placement>) {
    match which {
        PublishedCycling::Base => (BASE, BOOST_YOUR_BASE.placements().to_vec()),
        PublishedCycling::Build => (BUILD, BUILD_YOUR_POWER_ZONES.placements().to_vec()),
        PublishedCycling::Peak => (PEAK, peak_your_power_zones()),
        PublishedCycling::Test => (POWER_ZONE_TEST, power_zone_test()),
    }
}

fn named(name: &str) -> Result<ExternalProgramme, SourceError> {
    let malformed = |error: &dyn std::fmt::Display| SourceError::Malformed {
        detail: error.to_string(),
    };
    Ok(ExternalProgramme::new(
        Provider::try_from(PELOTON.to_owned()).map_err(|error| malformed(&error))?,
        ProgrammeName::try_from(name.to_owned()).map_err(|error| malformed(&error))?,
    ))
}

impl PublishedProgrammes for PelotonProgrammes<'_> {
    async fn read(&self, which: PublishedCycling) -> Result<PublishedRead, SourceError> {
        let (name, placements) = skeleton(which);
        let fetched = fetch(self.classes, &placements).await?;
        let cells = fetched
            .iter()
            .map(|(at, classes)| {
                (
                    *at,
                    PublishedCell {
                        ride: session(*at, classes),
                        measures_ftp: classes.iter().any(|class| class.is_ftp_test),
                    },
                )
            })
            .collect();
        Ok(PublishedRead {
            from: named(name)?,
            programme: programme(&fetched),
            cells,
        })
    }

    fn which(&self, programme: &ExternalProgramme) -> Option<PublishedCycling> {
        identify(programme)
    }
}

/// Which programme a provenance names, by the names Peloton gives them.
fn identify(programme: &ExternalProgramme) -> Option<PublishedCycling> {
    if programme.provider().as_str() != PELOTON {
        return None;
    }
    match programme.name().as_str() {
        BASE => Some(PublishedCycling::Base),
        BUILD => Some(PublishedCycling::Build),
        PEAK => Some(PublishedCycling::Peak),
        POWER_ZONE_TEST => Some(PublishedCycling::Test),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Read and identified under one name**, so a mesocycle committed from a
    /// programme is recognised as that programme when the next is chosen.
    #[test]
    fn every_programme_is_identified_by_the_name_it_is_read_under() {
        for which in [
            PublishedCycling::Base,
            PublishedCycling::Build,
            PublishedCycling::Peak,
            PublishedCycling::Test,
        ] {
            let (name, placements) = skeleton(which);
            assert!(!placements.is_empty(), "{name} places no class");
            let from = named(name).expect("a programme name is valid");
            assert_eq!(identify(&from), Some(which), "{name}");
        }
    }
}
