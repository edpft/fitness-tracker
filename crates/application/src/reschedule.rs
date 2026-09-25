//! The plan as it now stands, for everything that reads one.
//!
//! **One plan, and it is the rescheduled one** (#177). The operator,
//! 2026-09-21, asked whether the plan as authored should still be shown:
//! *"Okay, we could see what we originally planned, but that's not what we
//! did, so why should we care?"* So `fitness next`, the prescriber, `cycling
//! next` and `plan show` all read through these, and none of them can see the
//! authored dates by accident.
//!
//! **Stores over stores rather than a rescheduled table.** Which weeks were
//! lost is a function of the record, and a stored copy of it would stand wrong
//! the day a late workout landed (see [`crate::microcycle`]). So the authored
//! plan stays exactly as authored, and these answer every date from
//! [`domain::planner::rescheduled`] applied on the way out.
//!
//! **Identities are the authored mesocycles'.** Rescheduling moves a mesocycle
//! and does not make a new one, so a prescription issued against the entry
//! test before it was lost still belongs to the entry test after. The store is
//! asked for the id by the mesocycle's authored start, which only that
//! mesocycle covers.

use domain::{
    cycling::{CyclingMesocycle, CyclingMesocycleId},
    plan::{Occupies, Plan, PlanId, PlanName, PlanWindow, Programme},
    planner,
    prescription::{GymMesocycle, MesocycleId},
    schedule::Diary,
};
use jiff::civil::Date;

use crate::{CyclingMesocycleStore, MesocycleStore, PlanStore, StoreError};

/// Which weeks were lost, and the diary a moved mesocycle is read against.
///
/// What [`crate::microcycle::Standing`] hands every other reader.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reschedule {
    lost: Vec<Date>,
    diary: Diary,
}

impl Reschedule {
    #[must_use]
    pub const fn new(lost: Vec<Date>, diary: Diary) -> Self {
        Self { lost, diary }
    }

    /// Nothing lost: every plan reads exactly as authored.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The Mondays of the weeks re-run.
    #[must_use]
    pub fn lost(&self) -> &[Date] {
        &self.lost
    }

    fn apply(&self, plan: &Plan) -> Result<Plan, StoreError> {
        planner::rescheduled(plan, &self.lost, &self.diary).map_err(|error| StoreError::Corrupt {
            detail: format!("{} will not reschedule: {error}", plan.name()),
        })
    }
}

/// A store, answering as the plan now stands.
///
/// `plans` is where the whole plan is read from, because where one mesocycle
/// now falls depends on every mesocycle before it; `store` is the discipline's
/// own, and is asked only for identities.
pub struct Rescheduled<S, P> {
    store: S,
    plans: P,
    reschedule: Reschedule,
}

impl<S, P> Rescheduled<S, P> {
    pub const fn new(store: S, plans: P, reschedule: Reschedule) -> Self {
        Self {
            store,
            plans,
            reschedule,
        }
    }
}

impl<S: Sync, P: PlanStore + Sync> Rescheduled<S, P> {
    /// Every plan in force, as authored and as it now stands.
    async fn both(&self) -> Result<Vec<(Plan, Plan)>, StoreError> {
        let mut both = Vec::new();
        for window in self.plans.windows().await? {
            let Some(authored) = self.plans.named(window.name()).await? else {
                continue;
            };
            let now = self.reschedule.apply(&authored)?;
            both.push((authored, now));
        }
        Ok(both)
    }
}

/// Which mesocycle to answer with, from a programme as it now stands.
#[derive(Debug, Clone, Copy)]
enum Pick {
    /// The one whose span covers the date.
    On(Date),
    /// The latest to have finished by the date.
    Preceding(Date),
    /// The first to start after the date.
    Following(Date),
}

impl Pick {
    fn find<M: Occupies>(self, programme: &Programme<M>) -> Option<usize> {
        let spans: Vec<_> = programme.mesocycles().map(Occupies::span).collect();
        match self {
            Self::On(date) => spans.iter().position(|span| span.covers(date)),
            Self::Preceding(date) => spans.iter().rposition(|span| span.end() <= date),
            Self::Following(date) => spans.iter().position(|span| span.start() > date),
        }
    }
}

/// The authored start and the rescheduled mesocycle at one position.
fn located<M: Occupies + Clone>(
    authored: &Programme<M>,
    now: &Programme<M>,
    pick: Pick,
) -> Option<(Date, M)> {
    let at = pick.find(now)?;
    let start = authored.mesocycles().nth(at)?.span().start();
    let moved = now.mesocycles().nth(at)?.clone();
    Some((start, moved))
}

fn unfound(plan: &PlanName, start: Date) -> StoreError {
    StoreError::Corrupt {
        detail: format!(
            "{plan} holds no mesocycle starting on {start}, which it was authored with"
        ),
    }
}

impl<S: MesocycleStore + Sync, P: PlanStore + Sync> Rescheduled<S, P> {
    async fn gym(
        &self,
        pick: Pick,
    ) -> Result<Option<(MesocycleId, PlanName, GymMesocycle)>, StoreError> {
        for (authored, now) in self.both().await? {
            let (Some(authored_gym), Some(now_gym)) = (authored.gym(), now.gym()) else {
                continue;
            };
            let Some((start, moved)) = located(authored_gym, now_gym, pick) else {
                continue;
            };
            let (id, name, _) = self
                .store
                .on(start)
                .await?
                .ok_or_else(|| unfound(authored.name(), start))?;
            return Ok(Some((id, name, moved)));
        }
        Ok(None)
    }
}

impl<S: MesocycleStore + Sync, P: PlanStore + Sync> MesocycleStore for Rescheduled<S, P> {
    async fn on(
        &self,
        date: Date,
    ) -> Result<Option<(MesocycleId, PlanName, GymMesocycle)>, StoreError> {
        self.gym(Pick::On(date)).await
    }

    async fn preceding(
        &self,
        date: Date,
    ) -> Result<Option<(MesocycleId, PlanName, GymMesocycle)>, StoreError> {
        // **A plan with nothing before the date still has a predecessor
        // somewhere**: the mesocycle at the very front of a plan inherits from
        // the plan before it, which rescheduling does not touch.
        match self.gym(Pick::Preceding(date)).await? {
            Some(found) => Ok(Some(found)),
            None => self.store.preceding(date).await,
        }
    }

    async fn following(
        &self,
        date: Date,
    ) -> Result<Option<(MesocycleId, PlanName, GymMesocycle)>, StoreError> {
        self.gym(Pick::Following(date)).await
    }
}

impl<S: CyclingMesocycleStore + Sync, P: PlanStore + Sync> Rescheduled<S, P> {
    async fn cycling(
        &self,
        pick: Pick,
    ) -> Result<Option<(CyclingMesocycleId, PlanName, CyclingMesocycle)>, StoreError> {
        for (authored, now) in self.both().await? {
            let (Some(authored_bike), Some(now_bike)) = (authored.cycling(), now.cycling()) else {
                continue;
            };
            let Some((start, moved)) = located(authored_bike, now_bike, pick) else {
                continue;
            };
            let (id, name, _) = self
                .store
                .on(start)
                .await?
                .ok_or_else(|| unfound(authored.name(), start))?;
            return Ok(Some((id, name, moved)));
        }
        Ok(None)
    }
}

impl<S: CyclingMesocycleStore + Sync, P: PlanStore + Sync> CyclingMesocycleStore
    for Rescheduled<S, P>
{
    async fn on(
        &self,
        date: Date,
    ) -> Result<Option<(CyclingMesocycleId, PlanName, CyclingMesocycle)>, StoreError> {
        self.cycling(Pick::On(date)).await
    }

    async fn following(
        &self,
        date: Date,
    ) -> Result<Option<(CyclingMesocycleId, PlanName, CyclingMesocycle)>, StoreError> {
        self.cycling(Pick::Following(date)).await
    }
}

/// Plans, as they now stand.
pub struct ReschedulingPlans<P> {
    plans: P,
    reschedule: Reschedule,
}

impl<P> ReschedulingPlans<P> {
    pub const fn new(plans: P, reschedule: Reschedule) -> Self {
        Self { plans, reschedule }
    }
}

impl<P: PlanStore + Sync> PlanStore for ReschedulingPlans<P> {
    async fn windows(&self) -> Result<Vec<PlanWindow>, StoreError> {
        let mut windows = Vec::new();
        for window in self.plans.windows().await? {
            match self.named(window.name()).await? {
                Some(plan) => windows.push(plan.window()),
                None => windows.push(window),
            }
        }
        Ok(windows)
    }

    async fn named(&self, name: &PlanName) -> Result<Option<Plan>, StoreError> {
        self.plans
            .named(name)
            .await?
            .map(|plan| self.reschedule.apply(&plan))
            .transpose()
    }

    /// **Authoring writes what it is given**, which is always a plan someone
    /// stated. Rescheduling is a reading of the record, and writing it back
    /// would store a derived position the record may later contradict.
    async fn author(&self, plan: &Plan) -> Result<PlanId, StoreError> {
        self.plans.author(plan).await
    }

    async fn commit(
        &self,
        plan: &PlanName,
        gym: &[GymMesocycle],
        cycling: &[CyclingMesocycle],
    ) -> Result<(), StoreError> {
        self.plans.commit(plan, gym, cycling).await
    }
}
