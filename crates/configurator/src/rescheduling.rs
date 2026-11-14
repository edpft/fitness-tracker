//! The plan as it now stands, for every command that reads one (#177).
//!
//! **One place that works out which weeks were lost**, so that `fitness next`,
//! `gym next`, `mesocycle show` and `cycling next` cannot disagree about which
//! microcycle this week is. Each reads the store and nothing else: collecting
//! from a source is the porcelain's job, and a command asked where the plan
//! stands answers from what is already landed.
//!
//! **What is read through the plan as it stands reads the walk** (#809): the
//! performed sessions it is walked against, and the calendars its term is
//! read from. [`Walk`] is that walk, and [`Plans`] and [`GymMesocycles`] name
//! it in their types.

use application::{
    DiaryStore as _, HolidayCalendar as _, StoreError,
    microcycle::{Standing, Term},
    next::StandingReader,
    reschedule::{Reschedule, Rescheduled, ReschedulingPlans},
};
use domain::{normalised::OperatorZone, schedule::DayPart};
use infrastructure::{
    SqliteCalendarStore, SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore,
    SqliteCyclingDeliveryStore, SqliteDiaryStore, SqliteGymMesocycleStore, SqlitePlanStore,
    SqlitePrescriptionDeliveryStore,
};

use crate::{
    graph::reads,
    next::{NextFailure, destination},
    store::Store,
};

/// The walk through the plan, over this build's stores.
pub(crate) type Microcycle = application::microcycle::Microcycle<
    SqliteDiaryStore,
    SqlitePlanStore,
    SqlitePrescriptionDeliveryStore,
    SqliteCyclingDeliveryStore,
    SqliteCanonicalGymSessionStore,
    SqliteCanonicalCyclingSessionStore,
>;

reads! {
    /// The walk through the plan, within the term the calendars give it.
    pub struct Walk {
        microcycle: Microcycle,
        diary: SqliteDiaryStore,
        calendars: SqliteCalendarStore,
    }
}

/// The plans, as they now stand: what Plan reads, so every mesocycle a chart
/// is read over or an aim is drawn across has the days Plan shows (#800).
///
/// A rerun test week is in the mesocycle it reruns, though what was written
/// when that mesocycle started gave it one week fewer.
pub type Plans = ReschedulingPlans<SqlitePlanStore, Walk>;

/// The gym's mesocycles, as the plan now stands.
pub type GymMesocycles = Rescheduled<SqliteGymMesocycleStore, SqlitePlanStore, Walk>;

impl Walk {
    /// The walk over `store`, within the term the calendars give it as last
    /// served.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the build names no destination for a discipline.
    pub(crate) async fn over(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        let pool = store.pool();
        let mut walk = Self {
            microcycle: Microcycle::new(
                application::microcycle::MicrocyclePorts {
                    diary: SqliteDiaryStore::new(pool.clone()),
                    plans: SqlitePlanStore::new(pool.clone(), zone.clone()),
                    gym_deliveries: SqlitePrescriptionDeliveryStore::new(pool.clone()),
                    cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
                    gym_performed: SqliteCanonicalGymSessionStore::new(pool.clone()),
                    cycling_performed: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
                },
                destination(domain::planner::Discipline::Gym)?,
                destination(domain::planner::Discipline::Cycling)?,
            ),
            diary: SqliteDiaryStore::new(pool.clone()),
            calendars: SqliteCalendarStore::new(pool.clone()),
        };
        // **Not where either calendar has not been fetched yet** (#732).
        // Nothing starts again then, and a re-run lengthens its mesocycle as
        // it did before.
        if let Ok(holidays) = walk.calendars.holidays().await {
            walk.microcycle = walk.microcycle.within(Term {
                holidays,
                identify: infrastructure::peloton::provider::identify,
            });
        }
        Ok(walk)
    }

    /// Which weeks were lost, as of now in the operator's zone.
    ///
    /// **Now, whatever date a command was asked about.** Which weeks were lost
    /// is a fact about the record, and the record holds nothing from the
    /// future — so asking about next Friday reads the same lost weeks as asking
    /// about today.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub(crate) async fn reschedule(&self, zone: &OperatorZone) -> Result<Reschedule, StoreError> {
        let here = jiff::Timestamp::now().to_zoned(zone.as_time_zone());
        let standing = self
            .microcycle
            .standing(DayPart::containing(here.datetime()))
            .await?;
        Ok(standing.reschedule(self.diary.diary().await?))
    }
}

impl StandingReader for Walk {
    async fn standing(&self, now: DayPart) -> Result<Standing, StoreError> {
        self.microcycle.standing(now).await
    }
}

/// The plans as they now stand, walked over `store`.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub async fn plans(store: &Store, zone: &OperatorZone) -> Result<Plans, NextFailure> {
    let walk = Walk::over(store, zone).await?;
    Ok(Plans::walked(
        SqlitePlanStore::new(store.pool().clone(), zone.clone()),
        walk.reschedule(zone).await?,
        &walk,
    ))
}

/// The gym's mesocycles as the plan now stands, walked over `store`.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub async fn gym(store: &Store, zone: &OperatorZone) -> Result<GymMesocycles, NextFailure> {
    let walk = Walk::over(store, zone).await?;
    Ok(GymMesocycles::walked(
        SqliteGymMesocycleStore::new(store.pool().clone(), zone.clone()),
        SqlitePlanStore::new(store.pool().clone(), zone.clone()),
        walk.reschedule(zone).await?,
        &walk,
    ))
}
