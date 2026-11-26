//! Riding a published programme's shape on other classes (#436).
//!
//! **The operator, 2026-11-25:** *"The first time we programme a specific
//! programme like Build, we should programme it using the programme classes.
//! The next time we should substitute with non programme classes that I haven't
//! ridden in this cycle. We should keep doing that until we'd start reusing
//! classes in this cycle. Then we start again."* Re-riding a class is expected;
//! the cycle is there to lengthen the gap.
//!
//! **Four rounds, and then a new cycle.** Simulated over the 4×2 mesocycles the
//! macrocycle commits, the fourth round of each still had every slot within
//! nine minutes of work of the class it replaced, and Peak 2 began to slip on
//! the fifth. The operator settled on four from those figures.
//!
//! **The cycle is read off the plans, never stored.** Which round a mesocycle
//! is follows from how many of its kind were committed before it, replayed from
//! the first: a kind reaching its fifth starts the cycle again. A row saying
//! which cycle it belongs to would be a second answer to that.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use domain::{
    cycling::{Apart, CyclingMesocycle, CyclingMicrocycle, PlannedRide, RideVenue, ZoneProfile},
    macrocycle::Cycling,
    sequence::NonEmpty,
};
use jiff::civil::Date;

use crate::{StandaloneClasses, published::BuildError};

/// How many times a kind of mesocycle is committed in one cycle: once on its
/// programme's own classes and three times on substitutes.
pub const ROUNDS: usize = 4;

/// Which round a mesocycle is in its cycle, and where the cycle began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Round {
    /// From one. The first round rides the programme's own classes.
    pub number: usize,
    /// The day the cycle's first mesocycle started: what has been ridden or
    /// planned since is not offered again.
    pub since: Date,
}

/// Whether a kind of mesocycle is ridden on substitutes in later rounds.
///
/// **The mesocycles taken from a programme's weeks.** A hold is chosen out of
/// the catalogue already, a recovery repeats one deload week on purpose, and
/// the FTP test is the same test wherever it is ridden.
const fn rotates(kind: Cycling) -> bool {
    matches!(
        kind,
        Cycling::Base1
            | Cycling::Base2
            | Cycling::Build
            | Cycling::Peak1
            | Cycling::Peak2
            | Cycling::Selection { .. }
    )
}

/// The round `next`, starting on `start`, would be, after `history`.
///
/// `history` is every cycling mesocycle committed before it, of whatever kind,
/// oldest first. `None` where `next` is not a kind that rotates.
#[must_use]
pub fn round(history: &[(Cycling, Date)], next: Cycling, start: Date) -> Option<Round> {
    if !rotates(next) {
        return None;
    }
    let mut counts: HashMap<Cycling, usize> = HashMap::new();
    let mut since = None;
    for (kind, began) in history {
        if !rotates(*kind) {
            continue;
        }
        if counts.get(kind).copied().unwrap_or_default() >= ROUNDS {
            counts.clear();
            since = None;
        }
        since.get_or_insert(*began);
        *counts.entry(*kind).or_default() += 1;
    }
    let taken = counts.get(&next).copied().unwrap_or_default();
    if taken >= ROUNDS {
        return Some(Round {
            number: 1,
            since: start,
        });
    }
    Some(Round {
        number: taken + 1,
        since: since.unwrap_or(start),
    })
}

/// `mesocycle` with each ride replaced by the closest standalone class not in
/// `taken`, each choice added to `taken`.
///
/// **Closest is the least work apart, then the least recovery apart**
/// ([`Apart`]); the class's reference breaks a tie, so the same store gives the
/// same mesocycle.
///
/// **A ride keeps its slot.** Its role, and the published session it was taken
/// from, are the ones it replaces — as the FTP test keeps the Base slot it
/// stands in. Only the class and what it prescribes change.
///
/// **Not replaced:** a session of more than one class, which is the FTP
/// warm-up and test, and any ride the catalogue offers nothing for, which
/// stays as published.
///
/// # Errors
///
/// [`BuildError`] if the catalogue cannot be read, or what is built is not a
/// mesocycle the domain accepts.
pub async fn substituted<C: StandaloneClasses + Sync>(
    mesocycle: &CyclingMesocycle,
    classes: &C,
    taken: &mut BTreeSet<RideVenue>,
) -> Result<CyclingMesocycle, BuildError> {
    let unbuildable = |error: &dyn std::fmt::Display| BuildError::Unbuildable {
        detail: error.to_string(),
    };
    let mut weeks = Vec::with_capacity(mesocycle.duration_weeks());
    for week in mesocycle.microcycles().iter() {
        let mut rides = BTreeMap::new();
        for (position, ride) in week.rides() {
            let replaced = if ride.at().count() == 1 && !ride.session().is_test() {
                closest(ride, classes, taken).await?
            } else {
                None
            };
            rides.insert(*position, replaced.unwrap_or_else(|| ride.clone()));
        }
        weeks.push(CyclingMicrocycle::new(rides).map_err(|error| unbuildable(&error))?);
    }
    CyclingMesocycle::new(
        mesocycle.provenance().clone(),
        mesocycle.start(),
        NonEmpty::new(weeks).map_err(|error| unbuildable(&error))?,
    )
    .map_err(|error| unbuildable(&error))
}

/// The ride `ride` becomes on the closest class not in `taken`, if there is one.
async fn closest<C: StandaloneClasses + Sync>(
    ride: &PlannedRide,
    classes: &C,
    taken: &mut BTreeSet<RideVenue>,
) -> Result<Option<PlannedRide>, BuildError> {
    let from = ZoneProfile::of([ride.session().ride()]);
    let chosen = classes
        .like(ride.at().first())
        .await?
        .into_iter()
        .filter(|candidate| !taken.contains(&candidate.at))
        .min_by(|a, b| {
            let apart = |candidate: &crate::Standalone| {
                Apart::of(&ZoneProfile::of([candidate.session.ride()]), &from)
            };
            apart(a)
                .cmp(&apart(b))
                .then_with(|| a.at.reference().cmp(b.at.reference()))
        });
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    taken.insert(chosen.at.clone());
    let at = NonEmpty::of(chosen.at, Vec::new());
    Ok(Some(match ride.published() {
        Some(published) => PlannedRide::provided(chosen.session, at, published, ride.role()),
        None => PlannedRide::assembled(chosen.session, at, ride.role()),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    fn weeks_from(first: Date, kinds: &[Cycling]) -> Vec<(Cycling, Date)> {
        kinds
            .iter()
            .zip((0_i64..).step_by(4))
            .map(|(kind, weeks)| (*kind, first.saturating_add(jiff::Span::new().weeks(weeks))))
            .collect()
    }

    #[test]
    fn the_first_of_a_kind_rides_its_own_classes() {
        let start = date(2026, 9, 28);
        let found = round(&[], Cycling::Build, start);

        assert_eq!(
            found,
            Some(Round {
                number: 1,
                since: start
            })
        );
    }

    #[test]
    fn the_operators_plan_so_far_is_one_cycle() {
        // The test week, Build, then Peak in its two halves: each the first of
        // its kind, so all of them rode their own classes and the cycle began
        // with Build, the first that rotates.
        let history = weeks_from(
            date(2026, 9, 14),
            &[
                Cycling::Hold { holding: 0 },
                Cycling::Build,
                Cycling::Peak1,
                Cycling::Peak2,
            ],
        );
        let next = date(2026, 12, 21);

        assert_eq!(
            round(&history, Cycling::Build, next),
            Some(Round {
                number: 2,
                since: date(2026, 10, 12)
            })
        );
    }

    #[test]
    fn a_fifth_of_a_kind_starts_the_cycle_again() {
        let history = weeks_from(
            date(2026, 1, 5),
            &[
                Cycling::Build,
                Cycling::Peak1,
                Cycling::Build,
                Cycling::Build,
                Cycling::Peak1,
                Cycling::Build,
            ],
        );
        let next = date(2026, 12, 21);

        assert_eq!(
            round(&history, Cycling::Build, next),
            Some(Round {
                number: 1,
                since: next
            })
        );
        // Every kind starts again with it, not only the one that ran out.
        let restarted = [history, vec![(Cycling::Build, next)]].concat();
        let later = date(2027, 1, 18);
        assert_eq!(
            round(&restarted, Cycling::Peak1, later),
            Some(Round {
                number: 1,
                since: next
            })
        );
    }

    /// A catalogue offering the same classes whatever is asked.
    struct Offering(Vec<crate::Standalone>);

    impl StandaloneClasses for Offering {
        async fn like(
            &self,
            _venue: &RideVenue,
        ) -> Result<Vec<crate::Standalone>, crate::SourceError> {
            Ok(self.0.clone())
        }
    }

    fn session(
        zones: &[(domain::cycling::PowerZone, u64)],
    ) -> Result<domain::cycling::CyclingSession, Box<dyn std::error::Error>> {
        use domain::{
            cycling::{CyclingSession, Interval, Ride},
            measure::PositiveDuration,
        };
        let intervals = zones
            .iter()
            .map(|(zone, seconds)| {
                Ok(Interval::new(
                    *zone,
                    PositiveDuration::from_seconds(*seconds)?,
                ))
            })
            .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
        Ok(CyclingSession::new(
            PositiveDuration::from_seconds(600)?,
            Ride::Intervals(NonEmpty::new(intervals)?),
            Some(PositiveDuration::from_seconds(60)?),
        ))
    }

    fn test_session() -> Result<domain::cycling::CyclingSession, Box<dyn std::error::Error>> {
        use domain::{
            cycling::{CyclingSession, Ride},
            measure::PositiveDuration,
        };
        Ok(CyclingSession::new(
            PositiveDuration::from_seconds(600)?,
            Ride::Effort(PositiveDuration::from_seconds(1200)?),
            None,
        ))
    }

    fn venue(reference: &str) -> Result<RideVenue, Box<dyn std::error::Error>> {
        Ok(RideVenue::new(reference, reference)?)
    }

    #[test]
    fn the_closest_untaken_class_replaces_a_ride_and_the_test_stays() {
        use domain::{
            cycling::{CyclingProvenance, PowerZone, SessionPosition},
            planner::{Relative, SessionRole},
        };
        let harder = SessionRole::new(Relative::Higher, Relative::Lower);
        let easier = SessionRole::new(Relative::Lower, Relative::Higher);
        let work = [(PowerZone::Two, 300), (PowerZone::Four, 900)];

        let zoned = PlannedRide::assembled(
            session(&work).expect("a session"),
            NonEmpty::of(venue("published").expect("a venue"), Vec::new()),
            easier,
        );
        let tested = PlannedRide::assembled(
            test_session().expect("a test"),
            NonEmpty::of(
                venue("warm-up").expect("a venue"),
                vec![venue("ftp-test").expect("a venue")],
            ),
            harder,
        );
        let week = CyclingMicrocycle::new(BTreeMap::from([
            (SessionPosition::new(1).expect("a position"), tested),
            (SessionPosition::new(2).expect("a position"), zoned),
        ]))
        .expect("a week");
        let mesocycle = CyclingMesocycle::new(
            CyclingProvenance::Assembled,
            date(2026, 12, 21),
            NonEmpty::of(week, Vec::new()),
        )
        .expect("a mesocycle");

        let candidate = |reference: &str, zones: &[(PowerZone, u64)]| crate::Standalone {
            at: venue(reference).expect("a venue"),
            session: session(zones).expect("a session"),
        };
        let offering = Offering(vec![
            // The same work, resting in Z1 rather than Z2: closest.
            candidate(
                "rests-differently",
                &[(PowerZone::One, 300), (PowerZone::Four, 900)],
            ),
            // A minute of the work in Z5 instead.
            candidate(
                "works-differently",
                &[
                    (PowerZone::Two, 300),
                    (PowerZone::Four, 840),
                    (PowerZone::Five, 60),
                ],
            ),
            // Identical, but already ridden this cycle.
            candidate("ridden", &work),
        ]);
        let mut taken = BTreeSet::from([venue("ridden").expect("a venue")]);

        let Ok(runtime) = tokio::runtime::Builder::new_current_thread().build() else {
            panic!("a current-thread runtime builds")
        };
        let substituted = runtime
            .block_on(substituted(&mesocycle, &offering, &mut taken))
            .expect("substitutes are chosen");

        let rides: Vec<&PlannedRide> = substituted.microcycles().first().rides().values().collect();
        let references = |ride: &PlannedRide| -> Vec<String> {
            ride.at()
                .iter()
                .map(|at| at.reference().to_owned())
                .collect()
        };
        assert_eq!(
            references(rides[0]),
            vec!["warm-up", "ftp-test"],
            "the test is not swapped"
        );
        assert_eq!(references(rides[1]), vec!["rests-differently"]);
        assert_eq!(
            rides[1].role(),
            easier,
            "a substitute keeps the slot's role"
        );
        assert!(
            taken.contains(&venue("rests-differently").expect("a venue")),
            "what is chosen is not offered again"
        );
    }

    #[test]
    fn holds_recoveries_and_tests_do_not_rotate() {
        let start = date(2026, 12, 21);
        assert_eq!(round(&[], Cycling::Recovery, start), None);
        assert_eq!(round(&[], Cycling::Hold { holding: 2 }, start), None);
    }
}
