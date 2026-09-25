//! Where every session of the current microcycle stands.
//!
//! **The question `fitness next` actually asks** (#185). It used to ask a
//! narrower one — was the slot before today accounted for — and the operator
//! found what that misses on his first run of the installed build:
//! *"Maybe we should always start from the beginning of the microcycle."* A week
//! reported from its first session says where the week is up to; one reported
//! from the last slot says only what happened last night.
//!
//! **Nothing here decides anything and nothing here writes.** It reads the
//! plan, the diary and the record, and hands back a state per session.
//! Delivering the first one still to be prescribed is the caller's, and
//! rescheduling what was lost is #177's.

use domain::{
    plan::{Plan, Span},
    planner::{
        self, Filled, MicrocycleState, Placed, Recorded, Rerun, SessionState, Unreschedulable,
    },
    schedule::{
        DayPart, Diary, Discipline, RecordedSession, ScheduledSlot, SessionRole, accounted,
    },
};
use jiff::civil::Date;

use crate::{
    CyclingDeliveryStore, DestinationName, DiaryStore, PerformedSessionLog, PlanStore,
    PrescriptionDeliveryStore, RiddenSessionLog, StoreError,
};

/// One session of the microcycle, and where it stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub slot: ScheduledSlot,
    /// Which of its discipline's sessions in this microcycle, from one.
    pub number: u8,
    /// Whether the plan has a session for the slot at all.
    ///
    /// A slot outside every mesocycle, or in a week its programme skips, is
    /// still the operator's time and still reported — but it is owed nothing,
    /// so the microcycle machine does not read it.
    pub programmed: bool,
    /// Whether the plan has a test in the slot: the 1RM test or the FTP test.
    pub test: bool,
    pub state: SessionState,
}

/// Where a plan stands on a day.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Standing {
    /// Every session of this week, in the order the week runs.
    pub sessions: Vec<Session>,
    /// Each earlier week since the plan began, and how its concurrent
    /// microcycle ended.
    pub weeks: Vec<(Date, MicrocycleState)>,
    /// The weeks that run again, and which discipline holds instead.
    ///
    /// What every other reader of the plan needs to see it as it now stands:
    /// hand it to [`planner::rescheduled`], or to the stores in
    /// [`crate::reschedule`].
    pub reruns: Vec<Rerun>,
    /// The days of the mesocycle running now, both disciplines' halves
    /// together, as the plan now stands: what says which phase of the
    /// macrocycle this is (#224).
    pub mesocycle: Option<Span>,
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
    /// **Asked what was ridden, not merely when.** A ride names the class it
    /// was ridden to, which says which session of the microcycle it *was*
    /// (#177); the gym's record has no such answer, which is why the two are
    /// different ports rather than one asked twice.
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
    L: RiddenSessionLog + Sync,
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

    /// Where the plan stands on `now`: every week since it began, and every
    /// session of this one.
    ///
    /// **Two machines, walked together** (#177). Each week before this one is
    /// read as a concurrent microcycle, and a week that lost its essential
    /// sessions is re-run — the plan is rescheduled so that the next week holds
    /// the same microcycle, and every mesocycle after it starts a week later.
    /// Then this week is read against the plan as it now stands.
    ///
    /// **Derived on every read, never stored**, as a ladder's position is. A
    /// workout that lands late and turns out to have been the entry test undoes
    /// the shift on the next run, where a stored shift would stand wrong for
    /// ever.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something
    /// unreadable, including a plan that will not reschedule.
    pub async fn standing(&self, now: DayPart) -> Result<Standing, StoreError> {
        let diary = self.ports.diary.diary().await?;
        let Some(authored) = self.plan_begun_by(now.date).await? else {
            return Ok(Standing::default());
        };

        let this_week = planner::commencing(now.date);
        let mut monday = authored.window().span().start();
        monday = planner::commencing(monday);

        let mut reruns: Vec<Rerun> = Vec::new();
        let mut weeks: Vec<(Date, MicrocycleState)> = Vec::new();
        while monday < this_week {
            let plan = rescheduled(&authored, &reruns, &diary)?;
            let sunday = monday
                .checked_add(jiff::Span::new().days(6))
                .unwrap_or(monday);
            let sessions = self
                .week_of(&plan, &diary, monday, now, sunday.min(now.date))
                .await?;
            let placed = placed(&sessions);
            let state = planner::microcycle_state(&placed);
            if let Some(rerun) = planner::rerun(monday, state, &placed) {
                reruns.push(rerun);
            }
            weeks.push((monday, state));
            let Ok(next) = monday.checked_add(jiff::Span::new().weeks(1)) else {
                break;
            };
            monday = next;
        }

        let plan = rescheduled(&authored, &reruns, &diary)?;
        let sessions = self.week_of(&plan, &diary, now.date, now, now.date).await?;
        let mesocycle = plan
            .mesocycle_on(now.date)
            .map(|mesocycle| mesocycle.span());
        Ok(Standing {
            sessions,
            weeks,
            reruns,
            mesocycle,
        })
    }

    /// Every session of the calendar week containing `containing`, with its
    /// state, against a plan already rescheduled.
    ///
    /// In the order the week runs. An empty answer is a week the diary says
    /// nothing about, which is a real state and not a fault. `performed_by` is
    /// the last day the record is read to: today for this week, and the week's
    /// own Sunday for one already over, so that a later week's session never
    /// answers for an earlier week's slot.
    async fn week_of(
        &self,
        plan: &Plan,
        diary: &Diary,
        containing: Date,
        now: DayPart,
        performed_by: Date,
    ) -> Result<Vec<Session>, StoreError> {
        let week = planner::week(plan, diary, containing);
        if week.is_empty() {
            return Ok(Vec::new());
        }

        let slots: Vec<ScheduledSlot> = week.iter().map(|planned| planned.slot).collect();
        let performed = self.performed(&week, performed_by).await?;
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
                programmed: planned.session.is_ok(),
                test: planned.session.as_ref().is_ok_and(Filled::is_test),
                state: planner::state_of(
                    DayPart::new(slot.date, slot.slot.part),
                    closes,
                    now,
                    diary,
                    recorded,
                ),
            });
        }

        Ok(sessions)
    }

    /// The plan that has begun by a date: the latest whose authored start is
    /// on or before it.
    ///
    /// **Not the one whose span covers it**, because the span is the one
    /// authored and rescheduling moves its end: a plan three weeks behind still
    /// answers for the three weeks past where it was written to finish. Two
    /// plans never overlap, so the latest to have started is the one in force.
    async fn plan_begun_by(&self, date: Date) -> Result<Option<Plan>, StoreError> {
        let Some(window) = self
            .ports
            .plans
            .windows()
            .await?
            .into_iter()
            .filter(|window| window.span().start() <= date)
            .max_by_key(|window| window.span().start())
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

    /// What the record holds for this microcycle, and what it says each
    /// session was.
    ///
    /// Bounded at `today` because a session cannot have been performed in the
    /// future, and asking beyond it would let next week's ride answer for this
    /// week's slot.
    ///
    /// **The gym's sessions go in unnamed.** A Hevy workout names the day it
    /// was done; what session of the microcycle it *was* is only knowable where
    /// it was performed against a prescription, and reading that join is work
    /// nothing yet needs — the fallback in [`accounted`] gives the gym exactly
    /// the behaviour it had before (#177).
    async fn performed(
        &self,
        week: &[planner::Planned<'_>],
        today: Date,
    ) -> Result<Vec<RecordedSession>, StoreError> {
        let Some(from) = week.iter().map(|planned| planned.slot.date).min() else {
            return Ok(Vec::new());
        };
        if from > today {
            return Ok(Vec::new());
        }

        let mut recorded: Vec<RecordedSession> = self
            .ports
            .gym_performed
            .dates_between(from, today)
            .await?
            .into_iter()
            .map(|date| RecordedSession::unnamed(date, Discipline::Gym))
            .collect();

        for ridden in self
            .ports
            .cycling_performed
            .ridden_between(from, today)
            .await?
        {
            recorded.push(ridden_as(week, &ridden).map_or_else(
                || RecordedSession::unnamed(ridden.on, Discipline::Cycling),
                |role| RecordedSession::named(ridden.on, Discipline::Cycling, role),
            ));
        }

        Ok(recorded)
    }
}

/// Which session of the microcycle a ride was, from the class it was ridden to.
///
/// **Identity, not resemblance.** A planned ride names the classes it is ridden
/// at and a performed one names the classes it was ridden at, in one vocabulary
/// (§ 11) — so this is a comparison of references the destination issued and
/// nothing here interprets them. The operator's own case: the 45 min Power Zone
/// Endurance Ride he rode on Wednesday 16 September is the week's *lower*
/// intensity session, whatever day it landed on, and the FTP test is still
/// owed.
///
/// `None` where nothing matches — a ride taken under his own steam, or one
/// normalised before the class was recorded — and the record then says only
/// when it was.
fn ridden_as(week: &[planner::Planned<'_>], ridden: &crate::RiddenSession) -> Option<SessionRole> {
    week.iter().find_map(|planned| match planned.session {
        Ok(Filled::Cycling { ride, .. }) => ride
            .at()
            .iter()
            .any(|venue| ridden.at.contains(venue))
            .then(|| ride.role()),
        Ok(Filled::Gym { .. } | Filled::CyclingHolding { .. }) | Err(_) => None,
    })
}

/// The plan as it now stands, as the store's error.
///
/// **A plan that authored should always reschedule** — moving a mesocycle later
/// and skipping a week in it only ever give its calendar room — so a refusal is
/// something in the store this program could not have meant.
fn rescheduled(plan: &Plan, reruns: &[Rerun], diary: &Diary) -> Result<Plan, StoreError> {
    planner::rescheduled(plan, reruns, diary).map_err(|error: Unreschedulable| StoreError::Corrupt {
        detail: format!("{} will not reschedule: {error}", plan.name()),
    })
}

/// The sessions the microcycle machine reads: the ones the plan programmes.
fn placed(sessions: &[Session]) -> Vec<Placed> {
    sessions
        .iter()
        .filter(|session| session.programmed)
        .map(|session| Placed {
            discipline: session.slot.discipline,
            role: session.slot.role,
            state: session.state,
            test: session.test,
        })
        .collect()
}
