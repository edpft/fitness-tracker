//! One activity opened from History, the whole of it (#772).

use domain::{
    canonical::Occurred,
    cycling::CanonicalCyclingSession,
    gym::{CanonicalGymSession, exercise::RepsExercise},
};
use jiff::civil::Date;

use super::{Number, Tested, days};
use crate::{
    error::StoreError,
    ports::{
        PerformedCyclingSessions, PerformedGymSessions, PlayedPadelSession, PlayedPadelSessions,
    },
};

/// One activity, the whole of it.
///
/// **The sessions boxed**: a cycling session holds three rides inline, and a
/// gym session and a padel session their clock and heart rate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// A gym session, and the lifts whose 1RM it tested.
    Gym {
        session: Box<CanonicalGymSession>,
        tested: Vec<RepsExercise>,
    },
    Cycling(Box<CanonicalCyclingSession>),
    Padel(Box<PlayedPadelSession>),
}

impl Activity {
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

/// The use case.
pub struct Opened<G, C, P, T> {
    gym: G,
    cycling: C,
    padel: P,
    tested: T,
}

crate::graph::through!(Opened<G, C, P, T>);

impl<G, C, P, T> Opened<G, C, P, T>
where
    G: PerformedGymSessions + Sync,
    C: PerformedCyclingSessions + Sync,
    P: PlayedPadelSessions + Sync,
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

    /// The activity `number` of those done `on`, where there is one,
    /// numbered as the list numbers it.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn activity(&self, on: Date, number: Number) -> Result<Option<Activity>, StoreError> {
        let tests = self.tested.between(on, on).await?;
        let mut read: Vec<(Occurred, Activity)> = self
            .gym
            .between(on, on)
            .await?
            .into_iter()
            .map(|session| {
                let occurred = session.occurred().clone();
                let tested = tests
                    .iter()
                    .filter(|(tested, _)| *tested == occurred)
                    .map(|(_, lift)| *lift)
                    .fold(Vec::new(), |mut lifts, lift| {
                        if !lifts.contains(&lift) {
                            lifts.push(lift);
                        }
                        lifts
                    });
                (
                    occurred,
                    Activity::Gym {
                        session: Box::new(session),
                        tested,
                    },
                )
            })
            .collect();
        read.extend(
            self.cycling
                .between(on, on)
                .await?
                .into_iter()
                .map(|session| {
                    (
                        Occurred::At(session.started_at().clone()),
                        Activity::Cycling(Box::new(session)),
                    )
                }),
        );
        read.extend(self.padel.between(on, on).await?.into_iter().map(|played| {
            (
                Occurred::At(played.started_at().clone()),
                Activity::Padel(Box::new(played)),
            )
        }));
        Ok(days(read)
            .into_iter()
            .flat_map(|day| day.activities)
            .find(|(numbered, _)| *numbered == number)
            .map(|(_, (_, activity))| activity))
    }
}
