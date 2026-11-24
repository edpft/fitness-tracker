//! The activities he has done, newest first and by day, a page at a time, and
//! one of them opened (#772, #817).
//!
//! **Activities, of which a session is one kind.** The operator, 2026-11-15:
//! *"The history holds activities, which could be gym sessions, cycling
//! sessions, padel matches, yoga, etc. Session is my word of a focused piece of
//! training, which could, in theory, contain more than one workout or ride or
//! whatever."* A gym or cycling session is the canonical one, so a ride's
//! warm-up and cool-down are already parts of it rather than activities of
//! their own; a padel match is the normalised one, which only the watch
//! records. Nothing measured is here (#749), and nothing is written.
//!
//! **A session is a period at a venue.** The operator, 2026-11-15: *"within
//! the bounded context of activities, and gym and Peloton sessions specific,
//! what I think about is a period of time that I was in that venue, doing some
//! form of exercise."* So a cycling session is listed as a Peloton session: it
//! *"could include yoga or mobility classes that I don't do on the bike"*. For
//! now it holds only the rides; bringing the other classes in is a change to
//! the model, not to this list.
//!
//! **A page of the list reads what the list shows, for the days it shows.**
//! The operator, 2026-11-15: *"The history page inherently "shows" the whole
//! history but that shouldn't mean that it queries the whole history every time
//! I open the page."* Opening one reads the whole of it ([`opened`]).

use domain::{canonical::Occurred, gym::exercise::RepsExercise};
use jiff::{Span, Timestamp, civil::Date};

use crate::{error::StoreError, ports::Activities};

pub mod opened;

/// How many days with an activity a page of the list holds.
const DAYS: usize = 20;

/// How far back each read reaches while a page has fewer than [`DAYS`].
const WEEKS: i64 = 8;

pub use crate::ports::Activity;

/// One day's activities, and where each is opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Day<A = Activity> {
    pub on: Date,
    /// Newest first, each with its [`Number`].
    pub activities: Vec<(Number, A)>,
}

/// One page of the list, newest day first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub days: Vec<Day>,
    /// The day the next page reads back from, and not including; `None` where
    /// this page reaches the first activity the record holds.
    pub before: Option<Date>,
}

/// Which of a day's activities this is, counted from the first done that day.
///
/// **Counted from the first**, so an activity done later the same day does not
/// renumber the ones before it. One with no stated time comes first, as
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

/// A day's activities are counted from one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a day's activities are counted from one")]
pub struct NoSuchNumber;

/// The gym sessions that tested a lift's 1RM (#517).
///
/// **[`crate::one_rep_max::OneRepMaxTests`]'s to answer**: a session is a test
/// where its prescription named it one, which is that use case's to read, not
/// this one's.
pub trait Tested {
    /// Each gym session between the two days, both included, that tested a
    /// lift, and the lift.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    fn between(
        &self,
        from: Date,
        to: Date,
    ) -> impl Future<Output = Result<Vec<(Occurred, RepsExercise)>, StoreError>> + Send;
}

/// The use case.
pub struct History<G, C, P, T> {
    gym: G,
    cycling: C,
    padel: P,
    tested: T,
}

crate::graph::through!(History<G, C, P, T>);

impl<G, C, P, T> History<G, C, P, T>
where
    G: Activities + Sync,
    C: Activities + Sync,
    P: Activities + Sync,
    T: Tested + Sync,
{
    pub const fn new(gym: G, cycling: C, padel: P, tested: T) -> Self {
        Self {
            gym,
            cycling,
            padel,
            tested,
        }
    }

    /// The newest [`DAYS`] days before `before` that hold an activity, newest
    /// first, each gym session marked with the 1RM tests it performed.
    ///
    /// **Read back [`WEEKS`] at a time** until the page is full or the record
    /// begins, so a page over a stretch with nothing in it reads further
    /// rather than coming back short.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn page(&self, before: Date) -> Result<Page, StoreError> {
        let mut to = before.yesterday().map_err(|_| beyond())?;
        let mut first: Option<Option<Date>> = None;
        let mut read = Vec::new();
        let mut days = loop {
            let from = to
                .checked_sub(Span::new().weeks(WEEKS))
                .and_then(Date::tomorrow)
                .map_err(|_| beyond())?;
            read.extend(self.gym.between(from, to).await?);
            read.extend(self.cycling.between(from, to).await?);
            read.extend(self.padel.between(from, to).await?);
            let days = days(read.clone());
            if days.len() > DAYS {
                break days;
            }
            let first = match first {
                Some(first) => first,
                None => *first.insert(self.first().await?),
            };
            match first {
                // `<=`: the first in UTC may be a day off the first by its own
                // zone, so one more window is read rather than one day missed.
                Some(first) if first <= from => to = from.yesterday().map_err(|_| beyond())?,
                _ => break days,
            }
        };
        let more = days.len() > DAYS;
        days.truncate(DAYS);
        let before = days.last().map(|day| day.on).filter(|_| more);
        if let (Some(newest), Some(oldest)) = (days.first(), days.last()) {
            let tests = self.tested.between(oldest.on, newest.on).await?;
            mark(&mut days, &tests);
        }
        Ok(Page {
            days: days
                .into_iter()
                .map(|day| Day {
                    on: day.on,
                    activities: day
                        .activities
                        .into_iter()
                        .map(|(number, (_, activity))| (number, activity))
                        .collect(),
                })
                .collect(),
            before,
        })
    }

    /// The day of the first activity of any kind.
    async fn first(&self) -> Result<Option<Date>, StoreError> {
        Ok([
            self.gym.first().await?,
            self.cycling.first().await?,
            self.padel.first().await?,
        ]
        .into_iter()
        .flatten()
        .min())
    }
}

/// Each tested lift on the gym session that tested it.
fn mark(days: &mut [Day<(Occurred, Activity)>], tests: &[(Occurred, RepsExercise)]) {
    for (occurred, activity) in days
        .iter_mut()
        .flat_map(|day| day.activities.iter_mut())
        .map(|(_, activity)| activity)
    {
        if let Activity::Gym { tested, .. } = activity {
            for (on, lift) in tests {
                if on == occurred && !tested.contains(lift) {
                    tested.push(*lift);
                }
            }
        }
    }
}

/// A date past either end of the calendar.
fn beyond() -> StoreError {
    StoreError::Corrupt {
        detail: "a day beyond the calendar".to_owned(),
    }
}

/// When an activity started, where a source stated a time.
const fn instant(occurred: &Occurred) -> Option<Timestamp> {
    match occurred {
        Occurred::At(started) => Some(started.instant()),
        Occurred::On(_) => None,
    }
}

/// The activities by day, newest day first and newest first within it, each
/// numbered from the first done that day.
///
/// **Stable**: activities at one moment keep the order they were read in, so
/// the list and an opened day number them alike.
fn days<A>(mut activities: Vec<(Occurred, A)>) -> Vec<Day<(Occurred, A)>> {
    activities.sort_by_key(|(occurred, _)| (occurred.day(), instant(occurred)));
    let mut days: Vec<Day<(Occurred, A)>> = Vec::new();
    for activity in activities {
        let on = activity.0.day();
        match days.last_mut() {
            Some(day) if day.on == on => {
                let number = day.activities.len().saturating_add(1);
                day.activities.push((numbered(number), activity));
            }
            _ => days.push(Day {
                on,
                activities: vec![(numbered(1), activity)],
            }),
        }
    }
    for day in &mut days {
        day.activities.reverse();
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
    use jiff::{
        ToSpan as _,
        civil::{Date, date},
    };

    use super::{Activity, DAYS, History, Number, Tested, opened};
    use crate::{
        PrescribedWorkoutId,
        error::StoreError,
        ports::{
            Activities, PerformedCyclingSessions, PerformedGymSessions, PlayedPadelMatch,
            PlayedPadelMatches,
        },
    };

    type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

    const ACCOUNT: NormalisedSessionId = NormalisedSessionId::FIRST;

    /// One kind's activities, as the list reads them.
    struct Listed(Vec<(Occurred, Activity)>);

    impl Activities for Listed {
        async fn between(
            &self,
            from: Date,
            to: Date,
        ) -> Result<Vec<(Occurred, Activity)>, StoreError> {
            Ok(self
                .0
                .iter()
                .filter(|(occurred, _)| (from..=to).contains(&occurred.day()))
                .cloned()
                .collect())
        }

        async fn first(&self) -> Result<Option<Date>, StoreError> {
            Ok(self.0.iter().map(|(occurred, _)| occurred.day()).min())
        }
    }

    /// The sessions that tested a lift.
    struct Tests(Vec<(Occurred, RepsExercise)>);

    impl Tested for Tests {
        async fn between(
            &self,
            from: Date,
            to: Date,
        ) -> Result<Vec<(Occurred, RepsExercise)>, StoreError> {
            Ok(self
                .0
                .iter()
                .filter(|(occurred, _)| (from..=to).contains(&occurred.day()))
                .cloned()
                .collect())
        }
    }

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
        async fn main_rides(&self) -> Result<Vec<CanonicalBikePlusRide>, StoreError> {
            Ok(Vec::new())
        }

        async fn efforts(&self) -> Result<Vec<CanonicalBikePlusRide>, StoreError> {
            Ok(Vec::new())
        }

        async fn tested(&self) -> Result<Vec<StartedAt>, StoreError> {
            Ok(Vec::new())
        }

        async fn between(
            &self,
            from: Date,
            to: Date,
        ) -> Result<Vec<CanonicalCyclingSession>, StoreError> {
            Ok(self
                .0
                .iter()
                .filter(|session| (from..=to).contains(&session.started_at().wall_clock().date()))
                .cloned()
                .collect())
        }
    }

    struct Padel(Vec<PlayedPadelMatch>);

    impl PlayedPadelMatches for Padel {
        async fn between(&self, from: Date, to: Date) -> Result<Vec<PlayedPadelMatch>, StoreError> {
            Ok(self
                .0
                .iter()
                .filter(|played| (from..=to).contains(&played.started_at.wall_clock().date()))
                .cloned()
                .collect())
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

    /// A gym session as the list shows it: one exercise of one set.
    fn listed_gym(occurred: Occurred) -> (Occurred, Activity) {
        (
            occurred,
            Activity::Gym {
                length: None,
                exercises: 1,
                sets: 1,
                heart_rate: None,
                tested: Vec::new(),
            },
        )
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
            sources: std::collections::BTreeSet::new(),
        };
        let exercise = CanonicalExercise::ForReps {
            identified: Attributed::new(
                Identified::Recorded(RepsExercise::FrontSquatBarbell),
                ACCOUNT,
            ),
            sets: NonEmpty::new(vec![set])?,
        };
        Ok(CanonicalGymSession::new(
            domain::canonical::Attributed::new(
                occurred,
                domain::canonical::NormalisedSessionId::FIRST,
            ),
            NonEmpty::new(vec![CanonicalItem::exercise(exercise)])?,
            None,
            None,
            Vec::new(),
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

    /// His autumn, as the list reads it: the FTP test on Sunday 25 October,
    /// padel on Tuesday 27th, the gym on Friday 30th at 19:30 where front
    /// squat's 1RM was tested, and on Sunday 1 November a gym session dated
    /// only to the day and a ride that evening.
    fn autumn() -> Fallible<History<Listed, Listed, Listed, Tests>> {
        let friday = Occurred::At(started(date(2026, 10, 30), 19, 30)?);
        Ok(History::new(
            Listed(vec![
                listed_gym(friday.clone()),
                listed_gym(Occurred::On(date(2026, 11, 1))),
            ]),
            Listed(vec![
                (
                    Occurred::At(started(date(2026, 10, 25), 11, 0)?),
                    Activity::Cycling {
                        length: Duration::from_seconds(3_600),
                        power: Watts::from_u32(200),
                        test: true,
                    },
                ),
                (
                    Occurred::At(started(date(2026, 11, 1), 18, 0)?),
                    Activity::Cycling {
                        length: Duration::from_seconds(3_600),
                        power: Watts::from_u32(140),
                        test: false,
                    },
                ),
            ]),
            Listed(vec![(
                Occurred::At(started(date(2026, 10, 27), 20, 0)?),
                Activity::Padel {
                    length: PositiveDuration::from_seconds(5_400)?,
                    heart_rate: None,
                },
            )]),
            Tests(vec![(friday, RepsExercise::FrontSquatBarbell)]),
        ))
    }

    fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
        tokio::runtime::Builder::new_current_thread().build()
    }

    fn shown(page: &super::Page) -> Vec<(Date, Vec<(usize, &'static str)>)> {
        page.days
            .iter()
            .map(|day| {
                (
                    day.on,
                    day.activities
                        .iter()
                        .map(|(number, activity)| {
                            (
                                number.get(),
                                match activity {
                                    Activity::Gym { .. } if activity.is_test() => "1RM test",
                                    Activity::Gym { .. } => "gym",
                                    Activity::Cycling { .. } if activity.is_test() => "FTP test",
                                    Activity::Cycling { .. } => "cycling",
                                    Activity::Padel { .. } => "padel",
                                },
                            )
                        })
                        .collect(),
                )
            })
            .collect()
    }

    /// **Newest day first, newest first within it, padel among them, and each
    /// test marked**: Sunday's ride above Sunday's gym, each numbered from the
    /// first done that day, the gym with no time first; and only Friday's
    /// session holds a 1RM test.
    #[test]
    fn days_are_newest_first_and_tests_are_marked() {
        runtime().expect("a runtime").block_on(async {
            let history = autumn().expect("his autumn");
            let page = history.page(date(2026, 11, 13)).await.expect("read");

            assert_eq!(
                shown(&page),
                vec![
                    (date(2026, 11, 1), vec![(2, "cycling"), (1, "gym")]),
                    (date(2026, 10, 30), vec![(1, "1RM test")]),
                    (date(2026, 10, 27), vec![(1, "padel")]),
                    (date(2026, 10, 25), vec![(1, "FTP test")]),
                ]
            );
            assert_eq!(page.before, None, "the whole record is one page");
        });
    }

    /// **Nothing on or after the day asked before**, so a ride the record
    /// dates tomorrow waits.
    #[test]
    fn a_page_stops_before_its_day() {
        runtime().expect("a runtime").block_on(async {
            let history = autumn().expect("his autumn");
            let page = history.page(date(2026, 10, 31)).await.expect("read");

            assert_eq!(
                page.days.first().map(|day| day.on),
                Some(date(2026, 10, 30))
            );
        });
    }

    /// **A page is [`DAYS`] days, read back across a stretch with nothing in
    /// it**: fifteen gym sessions every third day to 13 November, and ten a
    /// year before with nothing between. The first page is the fifteen and the
    /// newest five of the ten, and the next starts where it stopped and ends
    /// the record.
    #[test]
    fn a_page_reads_back_across_a_gap_and_says_where_the_next_starts() {
        runtime().expect("a runtime").block_on(async {
            let recent = (0..15).map(|n| date(2026, 11, 13) - (3 * n).days());
            let year_before = (0..10).map(|n| date(2025, 11, 13) - (3 * n).days());
            let history = History::new(
                Listed(
                    recent
                        .chain(year_before)
                        .map(|on| listed_gym(Occurred::On(on)))
                        .collect(),
                ),
                Listed(Vec::new()),
                Listed(Vec::new()),
                Tests(Vec::new()),
            );

            let first = history.page(date(2026, 11, 14)).await.expect("read");
            assert_eq!(first.days.len(), DAYS);
            assert_eq!(
                first.days.first().map(|day| day.on),
                Some(date(2026, 11, 13))
            );
            assert_eq!(first.before, Some(date(2025, 11, 1)));
            assert_eq!(first.days.last().map(|day| day.on), first.before);

            let next = history
                .page(first.before.expect("more"))
                .await
                .expect("read");
            assert_eq!(
                next.days.iter().map(|day| day.on).collect::<Vec<_>>(),
                (5..10)
                    .map(|n| date(2025, 11, 13) - (3 * n).days())
                    .collect::<Vec<_>>()
            );
            assert_eq!(next.before, None);
        });
    }

    /// **A day and a number open the activity the list numbered**, the whole
    /// of it, and one past the day's last opens nothing.
    #[test]
    fn a_day_and_its_number_open_the_activity() {
        runtime().expect("a runtime").block_on(async {
            let friday = Occurred::At(started(date(2026, 10, 30), 19, 30).expect("Friday"));
            let opened = opened::Opened::new(
                Gym(vec![
                    gym(friday.clone()).expect("Friday's"),
                    gym(Occurred::On(date(2026, 11, 1))).expect("Sunday's"),
                ]),
                Rides(vec![CanonicalCyclingSession::Ride {
                    main: ride(date(2026, 11, 1), 18, "60 min Power Zone Endurance Ride")
                        .expect("a ride"),
                    cool_down: None,
                }]),
                Padel(Vec::new()),
                Tests(vec![(friday, RepsExercise::FrontSquatBarbell)]),
            );
            let first = Number::try_from(1).expect("one");
            let second = Number::try_from(2).expect("two");
            let third = Number::try_from(3).expect("three");

            let sunday = opened
                .activity(date(2026, 11, 1), second)
                .await
                .expect("read");
            assert!(
                matches!(&sunday, Some(opened::Activity::Cycling(ride)) if ride.working_ride().at().value().called() == "60 min Power Zone Endurance Ride"),
                "{sunday:?}"
            );
            assert_eq!(
                opened
                    .activity(date(2026, 11, 1), third)
                    .await
                    .expect("read"),
                None
            );
            let tested = opened
                .activity(date(2026, 10, 30), first)
                .await
                .expect("read");
            assert!(
                matches!(&tested, Some(opened::Activity::Gym { tested, .. }) if tested == &[RepsExercise::FrontSquatBarbell]),
                "{tested:?}"
            );
            assert!(Number::try_from(0).is_err());
        });
    }
}
