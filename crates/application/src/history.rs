//! The sessions he has done, newest first and by day, and one of them opened
//! (#772).
//!
//! **Gym sessions, cycling sessions and padel games, and nothing measured**:
//! the operator, 2026-11-10 (#749), *"Only sessions for now."* A gym or
//! cycling session is the canonical one, so a ride's warm-up and cool-down are
//! already parts of it rather than sessions of their own; a padel game is the
//! normalised one, which only the watch records. Nothing is written.

use domain::{
    canonical::Occurred,
    cycling::CanonicalCyclingSession,
    gym::{CanonicalGymSession, exercise::RepsExercise},
};
use jiff::{
    Timestamp,
    civil::{Date, date},
};

use crate::{
    error::StoreError,
    one_rep_max::Tests,
    ports::{PerformedCyclingSessions, PerformedGymSessions, PlayedPadelGame, PlayedPadelGames},
};

/// Before anything the record holds.
const EARLIEST: Date = date(2000, 1, 1);

/// One session done.
///
/// **The canonical ones boxed**: a cycling session holds three rides inline
/// and a gym session its clock and heart rate, and the feed moves them by the
/// hundred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    /// A gym session, and the lifts whose 1RM it tested.
    Gym {
        session: Box<CanonicalGymSession>,
        tested: Vec<RepsExercise>,
    },
    Cycling(Box<CanonicalCyclingSession>),
    Padel(PlayedPadelGame),
}

impl Session {
    /// The day it was done on, in the zone it was done in.
    pub fn on(&self) -> Date {
        match self {
            Self::Gym { session, .. } => session.occurred().day(),
            Self::Cycling(session) => session.started_at().wall_clock().date(),
            Self::Padel(game) => game.started_at.wall_clock().date(),
        }
    }

    /// When it started, where a source stated a time.
    const fn instant(&self) -> Option<Timestamp> {
        match self {
            Self::Gym { session, .. } => match session.occurred() {
                Occurred::At(started) => Some(started.instant()),
                Occurred::On(_) => None,
            },
            Self::Cycling(session) => Some(session.started_at().instant()),
            Self::Padel(game) => Some(game.started_at.instant()),
        }
    }

    /// Whether it was a test: an FTP test, or a gym session that tested a
    /// lift's 1RM.
    pub const fn is_test(&self) -> bool {
        match self {
            Self::Gym { tested, .. } => !tested.is_empty(),
            Self::Cycling(session) => matches!(**session, CanonicalCyclingSession::Test { .. }),
            Self::Padel(_) => false,
        }
    }
}

/// One day's sessions, and where each is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Day {
    pub on: Date,
    /// Newest first, each with its [`Number`].
    pub sessions: Vec<(Number, Session)>,
}

/// Which of a day's sessions this is, counted from the first done that day.
///
/// **Counted from the first**, so a session done later the same day does not
/// renumber the ones before it. A session with no stated time comes first, as
/// [`crate::ports::PerformedSessionLog`] orders them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Number(std::num::NonZeroUsize);

impl Number {
    pub const fn get(self) -> usize {
        self.0.get()
    }
}

impl TryFrom<usize> for Number {
    type Error = NoSuchNumber;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        std::num::NonZeroUsize::new(value)
            .map(Self)
            .ok_or(NoSuchNumber)
    }
}

/// A day's sessions are counted from one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a day's sessions are counted from one")]
pub struct NoSuchNumber;

/// The use case.
pub struct History<G, C, P> {
    gym: G,
    cycling: C,
    padel: P,
}

impl<G, C, P> History<G, C, P>
where
    G: PerformedGymSessions + Sync,
    C: PerformedCyclingSessions + Sync,
    P: PlayedPadelGames + Sync,
{
    pub const fn new(gym: G, cycling: C, padel: P) -> Self {
        Self {
            gym,
            cycling,
            padel,
        }
    }

    /// Every day through `today` that holds a session, newest first, each gym
    /// session marked with the 1RM tests it performed.
    ///
    /// **The tests are [`crate::one_rep_max::OneRepMaxTests`]'s**: a session
    /// is a test where its prescription named it one, which is that use case's
    /// to read, not this one's.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn days(&self, today: Date, tests: &[Tests]) -> Result<Vec<Day>, StoreError> {
        Ok(days(self.between(EARLIEST, today, tests).await?))
    }

    /// The session `number` of those done `on`, where there is one.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn session(
        &self,
        on: Date,
        number: Number,
        tests: &[Tests],
    ) -> Result<Option<Session>, StoreError> {
        Ok(days(self.between(on, on, tests).await?)
            .into_iter()
            .flat_map(|day| day.sessions)
            .find(|(numbered, _)| *numbered == number)
            .map(|(_, session)| session))
    }

    /// Every session done between the two days, both included.
    async fn between(
        &self,
        from: Date,
        to: Date,
        tests: &[Tests],
    ) -> Result<Vec<Session>, StoreError> {
        let mut sessions: Vec<Session> = self
            .gym
            .between(from, to)
            .await?
            .into_iter()
            .map(|session| Session::Gym {
                tested: tested(&session, tests),
                session: Box::new(session),
            })
            .collect();
        sessions.extend(
            self.cycling
                .all()
                .await?
                .into_iter()
                .map(|session| Session::Cycling(Box::new(session))),
        );
        sessions.extend(self.padel.all().await?.into_iter().map(Session::Padel));
        sessions.retain(|session| (from..=to).contains(&session.on()));
        Ok(sessions)
    }
}

/// The lifts whose 1RM a session tested: a test on its day, of a lift it
/// performed.
fn tested(session: &CanonicalGymSession, tests: &[Tests]) -> Vec<RepsExercise> {
    let on = session.occurred().day();
    tests
        .iter()
        .filter(|lift| lift.tests.iter().any(|(day, _)| *day == on))
        .filter(|lift| {
            session
                .exercises()
                .any(|exercise| exercise.exercise_key() == Some(lift.lift.as_str()))
        })
        .map(|lift| lift.lift)
        .collect()
}

/// The sessions by day, newest day first and newest first within it.
fn days(mut sessions: Vec<Session>) -> Vec<Day> {
    sessions.sort_by_key(|session| (session.on(), session.instant()));
    let mut days: Vec<Day> = Vec::new();
    for session in sessions {
        let on = session.on();
        match days.last_mut() {
            Some(day) if day.on == on => {
                let number = day.sessions.len().saturating_add(1);
                day.sessions.push((numbered(number), session));
            }
            _ => days.push(Day {
                on,
                sessions: vec![(numbered(1), session)],
            }),
        }
    }
    for day in &mut days {
        day.sessions.reverse();
    }
    days.reverse();
    days
}

/// A count that started at one and only grew.
fn numbered(count: usize) -> Number {
    Number(std::num::NonZeroUsize::new(count).unwrap_or(std::num::NonZeroUsize::MIN))
}

#[cfg(test)]
mod tests {
    use domain::{
        canonical::{Attributed, NormalisedSessionId, Occurred},
        cycling::{
            Cadence, CanonicalBikePlusRide, CanonicalCyclingSession, CanonicalRideRecord,
            Resistance, RideSample, RideVenue, Speed, Watts,
        },
        gym::{
            CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Identified,
            Performed, exercise::RepsExercise,
        },
        measure::{Duration, Metres, PositiveDuration, RepCount},
        normalised::{OperatorZone, StartedAt},
        prescription::DeliveryReference,
        sequence::NonEmpty,
    };
    use jiff::civil::{Date, date};

    use super::{History, Number, Session};
    use crate::one_rep_max::Tests;
    use crate::{
        PrescribedWorkoutId,
        error::StoreError,
        ports::{
            PerformedCyclingSessions, PerformedGymSessions, PlayedPadelGame, PlayedPadelGames,
        },
    };

    type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

    const ACCOUNT: NormalisedSessionId = NormalisedSessionId::FIRST;

    struct Gym(Vec<CanonicalGymSession>);

    impl PerformedGymSessions for Gym {
        async fn between(
            &self,
            from: Date,
            to: Date,
        ) -> Result<Vec<CanonicalGymSession>, StoreError> {
            Ok(self
                .0
                .iter()
                .filter(|session| (from..=to).contains(&session.occurred().day()))
                .cloned()
                .collect())
        }

        async fn fulfilling(
            &self,
            _prescription: PrescribedWorkoutId,
        ) -> Result<Option<(DeliveryReference, CanonicalGymSession)>, StoreError> {
            Ok(None)
        }
    }

    struct Rides(Vec<CanonicalCyclingSession>);

    impl PerformedCyclingSessions for Rides {
        async fn all(&self) -> Result<Vec<CanonicalCyclingSession>, StoreError> {
            Ok(self.0.clone())
        }
    }

    struct Padel(Vec<PlayedPadelGame>);

    impl PlayedPadelGames for Padel {
        async fn all(&self) -> Result<Vec<PlayedPadelGame>, StoreError> {
            Ok(self.0.clone())
        }
    }

    fn started(on: Date, hour: i8, minute: i8) -> Fallible<StartedAt> {
        let zone = OperatorZone::try_from("Europe/London".to_owned())?;
        let at = on
            .at(hour, minute, 0, 0)
            .to_zoned(zone.as_time_zone())?
            .timestamp();
        Ok(StartedAt::new(at, zone))
    }

    /// A gym session of one set of front squat, when it happened.
    fn gym(occurred: Occurred) -> Fallible<CanonicalGymSession> {
        let set = CanonicalSet {
            outcome: Attributed::new(Performed::Completed(Some(RepCount::new(5)?)), ACCOUNT),
            load: None,
            began: None,
            intensity: None,
            kind: None,
            rest_after: None,
        };
        let exercise = CanonicalExercise::ForReps {
            identified: Attributed::new(
                Identified::Recorded(RepsExercise::FrontSquatBarbell),
                ACCOUNT,
            ),
            sets: NonEmpty::new(vec![set])?,
        };
        Ok(CanonicalGymSession::new(
            occurred,
            NonEmpty::new(vec![CanonicalItem::Exercise(exercise)])?,
            None,
            None,
        ))
    }

    fn ride(on: Date, hour: i8, called: &str) -> Fallible<CanonicalBikePlusRide> {
        let sample = RideSample {
            at: Duration::ZERO,
            power: Watts::from_u32(140),
            cadence: Cadence::from_revolutions_per_minute(85),
            resistance: Resistance::from_percentage(40)?,
            speed: Speed::from_millimetres_per_hour(30_000_000),
        };
        Ok(CanonicalBikePlusRide::new(CanonicalRideRecord {
            started_at: Attributed::new(started(on, hour, 0)?, ACCOUNT),
            at: Attributed::new(RideVenue::new("class", called)?, ACCOUNT),
            duration: Attributed::new(Duration::from_seconds(2_700), ACCOUNT),
            distance: Attributed::new(Metres::from_millimetres(20_000_000), ACCOUNT),
            average_power: Attributed::new(Watts::from_u32(140), ACCOUNT),
            samples: Attributed::new(NonEmpty::new(vec![sample])?, ACCOUNT),
            heart_rate: None,
            heart_rate_declared_missing: None,
            heart_rate_device: None,
        }))
    }

    /// His autumn: the FTP test on Sunday 25 October, padel on Tuesday 27th,
    /// the gym on Friday 30th at 19:30, and on Sunday 1 November a gym session
    /// dated only to the day and a ride that evening.
    fn autumn() -> Fallible<History<Gym, Rides, Padel>> {
        let test = CanonicalCyclingSession::Test {
            warm_up: ride(date(2026, 10, 25), 11, "10 min FTP Warm Up Ride")?,
            effort: ride(date(2026, 10, 25), 12, "20 min FTP Test Ride")?,
            cool_down: Some(ride(date(2026, 10, 25), 13, "5 min Cool Down Ride")?),
        };
        let sunday = CanonicalCyclingSession::Ride {
            main: ride(date(2026, 11, 1), 18, "60 min Power Zone Endurance Ride")?,
            cool_down: None,
        };
        Ok(History::new(
            Gym(vec![
                gym(Occurred::At(started(date(2026, 10, 30), 19, 30)?))?,
                gym(Occurred::On(date(2026, 11, 1)))?,
            ]),
            Rides(vec![test, sunday]),
            Padel(vec![PlayedPadelGame {
                started_at: started(date(2026, 10, 27), 20, 0)?,
                duration: PositiveDuration::from_seconds(5_400)?,
                heart_rate: None,
            }]),
        ))
    }

    fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread().build()
    }

    /// **Newest day first, newest first within it, padel among them, and the
    /// test one session**:
    /// Sunday's ride above Sunday's gym, and each numbered from the first done
    /// that day, the gym with no time first.
    #[test]
    fn days_are_newest_first_and_a_test_is_one_session() {
        runtime().expect("a runtime").block_on(async {
            let history = autumn().expect("his autumn");
            let days = history.days(date(2026, 11, 12), &[]).await.expect("read");

            let shown: Vec<(Date, Vec<(usize, &str)>)> = days
                .iter()
                .map(|day| {
                    (
                        day.on,
                        day.sessions
                            .iter()
                            .map(|(number, session)| {
                                (
                                    number.get(),
                                    match session {
                                        Session::Gym { tested, .. } if tested.is_empty() => "gym",
                                        Session::Gym { .. } => "1RM test",
                                        Session::Cycling(_) => "cycling",
                                        Session::Padel(_) => "padel",
                                    },
                                )
                            })
                            .collect(),
                    )
                })
                .collect();
            assert_eq!(
                shown,
                vec![
                    (date(2026, 11, 1), vec![(2, "cycling"), (1, "gym")]),
                    (date(2026, 10, 30), vec![(1, "gym")]),
                    (date(2026, 10, 27), vec![(1, "padel")]),
                    (date(2026, 10, 25), vec![(1, "cycling")]),
                ]
            );
        });
    }

    /// **A test is marked as one**: the FTP test of 25 October, and the gym
    /// session of 30 October where front squat's 1RM was tested. A test of a
    /// lift the session did not perform marks nothing.
    #[test]
    fn tests_are_marked() {
        runtime().expect("a runtime").block_on(async {
            let history = autumn().expect("his autumn");
            let tests = [
                Tests {
                    lift: RepsExercise::FrontSquatBarbell,
                    tests: vec![(date(2026, 10, 30), 100_000)],
                },
                Tests {
                    lift: RepsExercise::DeadliftBarbell,
                    tests: vec![(date(2026, 11, 1), 150_000)],
                },
            ];
            let days = history
                .days(date(2026, 11, 12), &tests)
                .await
                .expect("read");

            let marked: Vec<(Date, Vec<bool>)> = days
                .iter()
                .map(|day| {
                    (
                        day.on,
                        day.sessions
                            .iter()
                            .map(|(_, session)| session.is_test())
                            .collect(),
                    )
                })
                .collect();
            assert_eq!(
                marked,
                vec![
                    (date(2026, 11, 1), vec![false, false]),
                    (date(2026, 10, 30), vec![true]),
                    (date(2026, 10, 27), vec![false]),
                    (date(2026, 10, 25), vec![true]),
                ]
            );
            let friday = days
                .iter()
                .find(|day| day.on == date(2026, 10, 30))
                .and_then(|day| day.sessions.first());
            assert!(
                matches!(friday, Some((_, Session::Gym { tested, .. })) if tested == &[RepsExercise::FrontSquatBarbell]),
                "{friday:?}"
            );
        });
    }

    /// **Nothing after today**, so a ride the record dates tomorrow waits.
    #[test]
    fn days_stop_at_today() {
        runtime().expect("a runtime").block_on(async {
            let history = autumn().expect("his autumn");
            let days = history.days(date(2026, 10, 31), &[]).await.expect("read");

            assert_eq!(days.first().map(|day| day.on), Some(date(2026, 10, 30)));
        });
    }

    /// **A day and a number open the session the feed numbered**, and one past
    /// the day's last opens nothing.
    #[test]
    fn a_day_and_its_number_open_the_session() {
        runtime().expect("a runtime").block_on(async {
            let history = autumn().expect("his autumn");
            let second = Number::try_from(2).expect("two");
            let third = Number::try_from(3).expect("three");

            let opened = history
                .session(date(2026, 11, 1), second, &[])
                .await
                .expect("read");
            assert!(
                matches!(&opened, Some(Session::Cycling(ride)) if ride.working_ride().at().value().called() == "60 min Power Zone Endurance Ride"),
                "{opened:?}"
            );
            assert_eq!(
                history
                    .session(date(2026, 11, 1), third, &[])
                    .await
                    .expect("read"),
                None
            );
            assert!(Number::try_from(0).is_err());
        });
    }
}
