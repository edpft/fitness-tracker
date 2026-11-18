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
    DiaryStore as _, HolidayCalendar as _, PlanStore as _, StoreError,
    microcycle::{Standing, Term},
    next::StandingReader,
    reschedule::{Reschedule, Rescheduled, WalkedPlans},
};
use domain::{
    normalised::OperatorZone,
    plan::Plan,
    schedule::{DayPart, Diary, Holidays},
};
use infrastructure::{
    SqliteCalendarStore, SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore,
    SqliteCyclingDeliveryStore, SqliteDiaryStore, SqliteGymMesocycleStore, SqlitePlanStore,
    SqlitePrescriptionDeliveryStore,
};

use crate::store::Spanned;
use crate::{
    graph::reads,
    next::{NextFailure, destination},
    store::Store,
};

/// The walk through the plan, over this build's stores.
pub(crate) type Microcycle = application::microcycle::Microcycle<
    Spanned<SqliteDiaryStore>,
    Spanned<SqlitePlanStore>,
    Spanned<SqlitePrescriptionDeliveryStore>,
    Spanned<SqliteCyclingDeliveryStore>,
    Spanned<SqliteCanonicalGymSessionStore>,
    Spanned<SqliteCanonicalCyclingSessionStore>,
>;

reads! {
    /// The walk through the plan, within the term the calendars give it.
    pub struct Walk {
        microcycle: Microcycle,
        plans: Spanned<SqlitePlanStore>,
        diary: Spanned<SqliteDiaryStore>,
        calendars: Spanned<SqliteCalendarStore>,
    }
}

/// What a walk read, and where it says the plan stands: each read once, for
/// whatever reads them after (#817).
///
/// **One walk for everything that reads the plan** (#830): what a chart's
/// ranges and aims are drawn across, and where the home page says the plan
/// stands, are read from this one walk.
pub struct Walked {
    /// The part of the day it was walked at.
    pub now: DayPart,
    /// The plans, as written and as they now stand.
    pub plans: Plans,
    pub diary: Diary,
    pub standing: Standing,
    pub reschedule: Reschedule,
}

/// The plans, as written and as they now stand: what Plan reads, so every
/// mesocycle a chart is read over or an aim is drawn across has the days Plan
/// shows (#800), as the walk read them (#817).
///
/// A rerun test week is in the mesocycle it reruns, though what was written
/// when that mesocycle started gave it one week fewer.
pub type Plans = WalkedPlans<Walk>;

/// The gym's mesocycles, as the plan now stands.
pub type GymMesocycles =
    Rescheduled<Spanned<SqliteGymMesocycleStore>, Spanned<SqlitePlanStore>, Walk>;

impl Walk {
    /// The walk over `store`, within the term the calendars give it as last
    /// served.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the build names no destination for a discipline.
    pub(crate) async fn over(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        let mut walk = Self {
            microcycle: Microcycle::new(
                application::microcycle::MicrocyclePorts {
                    diary: store.adapter(SqliteDiaryStore::new),
                    plans: store.adapter_with(SqlitePlanStore::new, zone.clone()),
                    gym_deliveries: store.adapter(SqlitePrescriptionDeliveryStore::new),
                    cycling_deliveries: store.adapter(SqliteCyclingDeliveryStore::new),
                    gym_performed: store.adapter(SqliteCanonicalGymSessionStore::new),
                    cycling_performed: store.adapter(SqliteCanonicalCyclingSessionStore::new),
                },
                destination(domain::planner::Discipline::Gym)?,
                destination(domain::planner::Discipline::Cycling)?,
            ),
            plans: store.adapter_with(SqlitePlanStore::new, zone.clone()),
            diary: store.adapter(SqliteDiaryStore::new),
            calendars: store.adapter(SqliteCalendarStore::new),
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
        Ok(self.walked(zone).await?.reschedule)
    }

    /// Which weeks were lost, as [`Self::reschedule`] says, with the plans
    /// and the diary it read to say so.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub(crate) async fn walked(&self, zone: &OperatorZone) -> Result<Walked, StoreError> {
        let here = jiff::Timestamp::now().to_zoned(zone.as_time_zone());
        self.walked_on(DayPart::containing(here.datetime())).await
    }

    /// Where the plan stands on `now`, and the plans and the diary read to
    /// say so.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub(crate) async fn walked_on(&self, now: DayPart) -> Result<Walked, StoreError> {
        let written = self.plans.plans().await?;
        let diary = self.diary.diary().await?;
        let standing = self
            .microcycle
            .standing_among(now, &written, &diary)
            .await?;
        let reschedule = standing.reschedule(diary.clone());
        Ok(Walked {
            now,
            plans: Plans::new(written, &reschedule, self.holidays().cloned(), self)?,
            reschedule,
            standing,
            diary,
        })
    }

    /// The school and public holidays the walk's term was read in, where
    /// the calendars could be read.
    pub(crate) fn holidays(&self) -> Option<&Holidays> {
        self.microcycle.holidays()
    }
}

impl StandingReader for Walk {
    async fn standing(&self, now: DayPart) -> Result<Standing, StoreError> {
        self.microcycle.standing(now).await
    }

    async fn standing_among(
        &self,
        now: DayPart,
        plans: &[Plan],
        diary: &Diary,
    ) -> Result<Standing, StoreError> {
        self.microcycle.standing_among(now, plans, diary).await
    }
}

/// The gym's mesocycles as the plan now stands, walked over `store`.
///
/// # Errors
///
/// [`NextFailure`] if the store cannot be read.
pub async fn gym(store: &Store, zone: &OperatorZone) -> Result<GymMesocycles, NextFailure> {
    let walk = Walk::over(store, zone).await?;
    Ok(GymMesocycles::walked(
        store.adapter_with(SqliteGymMesocycleStore::new, zone.clone()),
        store.adapter_with(SqlitePlanStore::new, zone.clone()),
        walk.reschedule(zone).await?,
        &walk,
    ))
}
