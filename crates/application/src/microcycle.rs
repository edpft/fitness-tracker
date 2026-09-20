//! Where every session of the current microcycle stands.
//!
//! **The question `fitness next` actually asks** (#185). It used to ask a
//! narrower one — was the slot before today accounted for — and the operator
//! found what that misses on his first run of the installed build:
//! *"Maybe we should always start from the begining of the microcycle."* A week
//! reported from its first session says where the week is up to; one reported
//! from the last slot says only what happened last night.
//!
//! **Nothing here decides anything and nothing here writes.** It reads the
//! plan, the diary and the record, and hands back a state per session.
//! Delivering the first one still to be prescribed is the caller's, and
//! rescheduling what was lost is #177's.

use std::collections::BTreeMap;

use domain::{
    plan::Plan,
    planner::{self, Recorded, SessionState},
    schedule::{DayPart, Discipline, ScheduledSlot, accounted},
};
use jiff::civil::Date;

use crate::{
    CyclingDeliveryStore, DestinationName, DiaryStore, PerformedSessionLog, PlanStore,
    PrescriptionDeliveryStore, StoreError,
};

/// One session of the microcycle, and where it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub slot: ScheduledSlot,
    /// Which of its discipline's sessions in this microcycle, from one.
    pub number: u8,
    pub state: SessionState,
}

impl Session {
    /// The moment its slot begins.
    #[must_use]
    pub const fn at(&self) -> DayPart {
        DayPart::new(self.slot.date, self.slot.slot.part)
    }
}

/// Everything the standing of a microcycle is read from.
///
/// **Two performed logs rather than one asked twice**, because each discipline
/// has its own record and the adapter that reads it is the one that knows how:
/// a gym session is a Hevy workout, a cycling session is two or three Peloton
/// rides, and the date is the one thing both can answer with.
pub struct MicrocyclePorts<D, P, G, C, Y, L> {
    pub diary: D,
    pub plans: P,
    /// Where a gym prescription's place at a destination is recorded.
    pub gym_deliveries: G,
    /// Where a delivered ride is recorded.
    pub cycling_deliveries: C,
    pub gym_performed: Y,
    pub cycling_performed: L,
}

/// Where the microcycle containing a date stands.
pub struct Microcycle<D, P, G, C, Y, L> {
    ports: MicrocyclePorts<D, P, G, C, Y, L>,
    /// Where the gym's sessions are put, so "prescribed" can be asked of it.
    gym_destination: DestinationName,
    /// Where rides are put.
    cycling_destination: DestinationName,
}

impl<D, P, G, C, Y, L> Microcycle<D, P, G, C, Y, L>
where
    D: DiaryStore + Sync,
    P: PlanStore + Sync,
    G: PrescriptionDeliveryStore + Sync,
    C: CyclingDeliveryStore + Sync,
    Y: PerformedSessionLog + Sync,
    L: PerformedSessionLog + Sync,
{
    /// **The destinations are arguments**, because which sink a discipline has
    /// is a fact about the build rather than about the core: the catalogue
    /// names one per discipline, and a use case inventing the string would be
    /// the second place that name lived.
    pub const fn new(
        ports: MicrocyclePorts<D, P, G, C, Y, L>,
        gym_destination: DestinationName,
        cycling_destination: DestinationName,
    ) -> Self {
        Self {
            ports,
            gym_destination,
            cycling_destination,
        }
    }

    /// Every session of the calendar week containing `now`, with its state.
    ///
    /// In the order the week runs. An empty answer is a week the diary says
    /// nothing about, which is a real state and not a fault.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something
    /// unreadable.
    pub async fn standing(&self, now: DayPart) -> Result<Vec<Session>, StoreError> {
        let diary = self.ports.diary.diary().await?;
        let Some(plan) = self.plan_covering(now.date).await? else {
            return Ok(Vec::new());
        };

        let week = planner::week(&plan, &diary, now.date);
        if week.is_empty() {
            return Ok(Vec::new());
        }

        let slots: Vec<ScheduledSlot> = week.iter().map(|planned| planned.slot).collect();
        let performed = self.performed(&slots, now.date).await?;
        let answered = accounted(&slots, &performed);

        // **What closes the last session's window comes from the diary**, not
        // from the end of the week: a Sunday session is still performable on
        // the Monday morning if nothing is slotted before then.
        let after = slots
            .last()
            .and_then(|last| diary.first_ordinary_after(last.date))
            .map(|slot| DayPart::new(slot.date, slot.slot.part));

        let mut sessions = Vec::with_capacity(week.len());
        for (at, planned) in week.iter().enumerate() {
            let slot = planned.slot;
            let closes = slots
                .get(at.saturating_add(1))
                .map_or(after, |next| Some(DayPart::new(next.date, next.slot.part)));

            let recorded = Recorded {
                prescribed: self.prescribed(slot).await?,
                performed: answered.get(at).copied().unwrap_or(false),
            };

            sessions.push(Session {
                slot,
                number: planned.number,
                state: planner::state_of(
                    DayPart::new(slot.date, slot.slot.part),
                    closes,
                    now,
                    &diary,
                    recorded,
                ),
            });
        }

        Ok(sessions)
    }

    /// The plan whose span covers a date, if one does.
    async fn plan_covering(&self, date: Date) -> Result<Option<Plan>, StoreError> {
        let Some(window) = self
            .ports
            .plans
            .windows()
            .await?
            .into_iter()
            .find(|window| window.span().covers(date))
        else {
            return Ok(None);
        };
        self.ports.plans.named(window.name()).await
    }

    /// Whether a session was put somewhere it could be performed (§ 12.1).
    ///
    /// **Delivered, not merely issued.** A gym prescription that was derived
    /// and never sent is not a session the operator can train from, and
    /// reporting it as prescribed would say the tool had done something it had
    /// not.
    async fn prescribed(&self, slot: ScheduledSlot) -> Result<bool, StoreError> {
        match slot.discipline {
            Discipline::Gym => Ok(self
                .ports
                .gym_deliveries
                .occupying(slot.date, &self.gym_destination)
                .await?
                .is_some()),
            Discipline::Cycling => Ok(self
                .ports
                .cycling_deliveries
                .delivered_for(slot.date, &self.cycling_destination)
                .await?
                .is_some()),
        }
    }

    /// Each discipline's session dates, from the microcycle's first slot to
    /// the day being asked about.
    ///
    /// Bounded at `today` because a session cannot have been performed in the
    /// future, and asking beyond it would let next week's ride answer for this
    /// week's slot.
    async fn performed(
        &self,
        slots: &[ScheduledSlot],
        today: Date,
    ) -> Result<BTreeMap<Discipline, Vec<Date>>, StoreError> {
        let mut performed = BTreeMap::new();
        let Some(from) = slots.iter().map(|slot| slot.date).min() else {
            return Ok(performed);
        };
        if from > today {
            return Ok(performed);
        }

        performed.insert(
            Discipline::Gym,
            self.ports.gym_performed.dates_between(from, today).await?,
        );
        performed.insert(
            Discipline::Cycling,
            self.ports
                .cycling_performed
                .dates_between(from, today)
                .await?,
        );
        Ok(performed)
    }
}
