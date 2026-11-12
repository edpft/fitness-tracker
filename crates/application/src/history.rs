//! The sessions he has done, newest first and by day, and one of them opened
//! (#772).
//!
//! **Gym sessions and rides, and nothing measured**: the operator, 2026-11-10
//! (#749), *"Only sessions for now."* Each is the canonical session, so a ride's
//! warm-up and cool-down are already parts of it rather than sessions of their
//! own. Nothing is written.

use domain::{canonical::Occurred, cycling::CanonicalCyclingSession, gym::CanonicalGymSession};
use jiff::{
    Timestamp,
    civil::{Date, date},
};

use crate::{
    error::StoreError,
    ports::{PerformedCyclingSessions, PerformedGymSessions},
};

/// Before anything the record holds.
const EARLIEST: Date = date(2000, 1, 1);

/// One session done.
///
/// **Both boxed**: a ride holds three rides inline and a gym session its
/// clock and heart rate, and the feed moves them by the hundred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    Gym(Box<CanonicalGymSession>),
    Cycling(Box<CanonicalCyclingSession>),
}

impl Session {
    /// The day it was done on, in the zone it was done in.
    pub fn on(&self) -> Date {
        match self {
            Self::Gym(session) => session.occurred().day(),
            Self::Cycling(session) => session.started_at().wall_clock().date(),
        }
    }

    /// When it started, where a source stated a time.
    const fn instant(&self) -> Option<Timestamp> {
        match self {
            Self::Gym(session) => match session.occurred() {
                Occurred::At(started) => Some(started.instant()),
                Occurred::On(_) => None,
            },
            Self::Cycling(session) => Some(session.started_at().instant()),
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
pub struct History<G, C> {
    gym: G,
    cycling: C,
}

impl<G, C> History<G, C>
where
    G: PerformedGymSessions + Sync,
    C: PerformedCyclingSessions + Sync,
{
    pub const fn new(gym: G, cycling: C) -> Self {
        Self { gym, cycling }
    }

    /// Every day through `today` that holds a session, newest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn days(&self, today: Date) -> Result<Vec<Day>, StoreError> {
        let mut sessions: Vec<Session> = self
            .gym
            .between(EARLIEST, today)
            .await?
            .into_iter()
            .map(|session| Session::Gym(Box::new(session)))
            .collect();
        sessions.extend(
            self.cycling
                .all()
                .await?
                .into_iter()
                .map(|session| Session::Cycling(Box::new(session)))
                .filter(|session| session.on() <= today),
        );
        Ok(days(sessions))
    }

    /// The session `number` of those done `on`, where there is one.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn session(&self, on: Date, number: Number) -> Result<Option<Session>, StoreError> {
        let mut sessions: Vec<Session> = self
            .gym
            .between(on, on)
            .await?
            .into_iter()
            .map(|session| Session::Gym(Box::new(session)))
            .collect();
        sessions.extend(
            self.cycling
                .all()
                .await?
                .into_iter()
                .map(|session| Session::Cycling(Box::new(session)))
                .filter(|session| session.on() == on),
        );
        Ok(days(sessions)
            .into_iter()
            .flat_map(|day| day.sessions)
            .find(|(numbered, _)| *numbered == number)
            .map(|(_, session)| session))
    }
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
        measure::{Duration, Metres, RepCount},
        normalised::{OperatorZone, StartedAt},
        prescription::DeliveryReference,
        sequence::NonEmpty,
    };
    use jiff::civil::{Date, date};

    use super::{History, Number, Session};
    use crate::{
        PrescribedWorkoutId,
        error::StoreError,
        ports::{PerformedCyclingSessions, PerformedGymSessions},
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

    /// His autumn: the FTP test on Sunday 25 October, the gym on Friday 30th at
    /// 19:30, and on Sunday 1 November a gym session dated only to the day and
    /// a ride that evening.
    fn autumn() -> Fallible<History<Gym, Rides>> {
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
        ))
    }

    fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread().build()
    }

    /// **Newest day first, newest first within it, and the test one session**:
    /// Sunday's ride above Sunday's gym, and each numbered from the first done
    /// that day, the gym with no time first.
    #[test]
    fn days_are_newest_first_and_a_test_is_one_session() {
        runtime().expect("a runtime").block_on(async {
            let history = autumn().expect("his autumn");
            let days = history.days(date(2026, 11, 12)).await.expect("read");

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
                                        Session::Gym(_) => "gym",
                                        Session::Cycling(_) => "cycling",
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
                    (date(2026, 10, 25), vec![(1, "cycling")]),
                ]
            );
        });
    }

    /// **Nothing after today**, so a ride the record dates tomorrow waits.
    #[test]
    fn days_stop_at_today() {
        runtime().expect("a runtime").block_on(async {
            let history = autumn().expect("his autumn");
            let days = history.days(date(2026, 10, 31)).await.expect("read");

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
                .session(date(2026, 11, 1), second)
                .await
                .expect("read");
            assert!(
                matches!(&opened, Some(Session::Cycling(ride)) if ride.working_ride().at().value().called() == "60 min Power Zone Endurance Ride"),
                "{opened:?}"
            );
            assert_eq!(
                history
                    .session(date(2026, 11, 1), third)
                    .await
                    .expect("read"),
                None
            );
            assert!(Number::try_from(0).is_err());
        });
    }
}
