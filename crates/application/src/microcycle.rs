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
        DayPart, Diary, Discipline, PartOfDay, RecordedSession, ScheduledSlot, SessionRole,
        accounted,
    },
};
use jiff::civil::{Date, DateTime};

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
    /// The Monday the current microcycle commences: the first week that has
    /// not ended, which is not always the one containing today (#281).
    pub commencing: Option<Date>,
    /// Every session of the current microcycle, in the order it runs.
    pub sessions: Vec<Session>,
    /// Each earlier microcycle since the plan began, and how it ended.
    pub weeks: Vec<(Date, MicrocycleState)>,
    /// The weeks that run again, and which discipline holds instead.
    ///
    /// What every other reader of the plan needs to see it as it now stands:
    /// hand it to [`planner::rescheduled`], or to the stores in
    /// [`crate::reschedule`].
    pub reruns: Vec<Rerun>,
    /// The days of the current microcycle's mesocycle, both disciplines' halves
    /// together, as the plan now stands: what says which phase of the
    /// macrocycle this is (#224).
    pub mesocycle: Option<Span>,
}

/// One week read against the plan, and when it ended if it has.
struct Week {
    sessions: Vec<Session>,
    /// Where the next microcycle's record starts: `None` while this one is
    /// still current.
    ended: Option<Boundary>,
}

/// One session the record holds, in the grain the record holds it.
///
/// **A day and a clock that may not be there.** The gym's record is the canonical
/// layer, which dates a visit to the finest grain any of its sources knew: 61 of
/// the operator's 389 are a day and no finer (#350). Everything this reading does
/// with a session either needs only the day — which slot it answers — or needs
/// the moment and has somewhere to go without it, which is what makes the clock
/// optional rather than the record incomplete.
#[derive(Debug, Clone, Copy)]
struct Account {
    on: Date,
    /// The wall-clock time it started, where a source recorded one.
    at: Option<DateTime>,
    session: RecordedSession,
}

/// Where one microcycle's record ends and the next one's begins.
///
/// **Two kinds, because a session stops being completable two ways** (#281).
/// Performing it settles it at that performance, so whatever is performed after
/// the last one — even in the same part of the day — is the next microcycle's.
/// A window closing settles it at the part of the day it closes on, and the
/// record knows a session's part only as [`DayPart::containing`] reads it.
#[derive(Debug, Clone, Copy)]
enum Boundary {
    After(DateTime),
    From(DayPart),
}

impl Boundary {
    /// Whether a session that happened at `occurred` is on the later side.
    ///
    /// **A session dated to a day and no finer is admitted only where the whole
    /// day is.** Both kinds of boundary fall inside a day — one at a performance
    /// and one at a part of it — so a day that contains the boundary contains
    /// moments on each side of it, and nothing in the record says which side this
    /// session was. Counting it would spend a microcycle on a session that may
    /// have belonged to the one before; leaving it out leaves a session owed that
    /// the operator can see he performed. The second error is the one he can
    /// correct.
    fn admits(self, on: Date, at: Option<DateTime>) -> bool {
        match (self, at) {
            (Self::After(performed), Some(at)) => at > performed,
            (Self::From(part), Some(at)) => DayPart::containing(at) >= part,
            (Self::After(performed), None) => on > performed.date(),
            (Self::From(part), None) => on > part.date,
        }
    }

    /// Whichever of the two comes later. A performance in a part of the day
    /// is later than that part beginning.
    fn later(self, other: Self) -> Self {
        let key = |boundary: Self| match boundary {
            Self::After(performed) => (DayPart::containing(performed), Some(performed)),
            Self::From(part) => (part, None),
        };
        if key(other) > key(self) { other } else { self }
    }

    fn date(self) -> Date {
        match self {
            Self::After(performed) => performed.date(),
            Self::From(part) => part.date,
        }
    }
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

    /// Where the plan stands on `now`: every microcycle since it began, and
    /// every session of the current one.
    ///
    /// **Two machines, walked together** (#177). Each microcycle that has ended
    /// is read as a concurrent microcycle, and one that lost its essential
    /// sessions is re-run — the plan is rescheduled so that the next week holds
    /// the same microcycle, and every mesocycle after it starts a week later.
    /// Then the first that has not ended is read against the plan as it now
    /// stands.
    ///
    /// **A microcycle ends once none of its sessions can still be completed**
    /// (operator, 2026-09-27, #281) — not at midnight on the Sunday. Riding the
    /// Sunday session, when it is the last one owed, makes the next week
    /// current that evening; not riding it leaves this one current until the
    /// Monday evening slot closes its window. A session performed belongs to
    /// the microcycle that was current when it was.
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

        let mut monday = planner::commencing(authored.window().span().start());
        let mut opens: Option<Boundary> = None;
        let mut reruns: Vec<Rerun> = Vec::new();
        let mut weeks: Vec<(Date, MicrocycleState)> = Vec::new();
        loop {
            let plan = rescheduled(&authored, &reruns, &diary)?;
            let week = self.week_of(&plan, &diary, monday, opens, now).await?;
            let Some(ended) = week.ended else {
                let mesocycle = plan
                    .mesocycle_on(monday)
                    .or_else(|| plan.mesocycle_on(now.date))
                    .map(|mesocycle| mesocycle.span());
                return Ok(Standing {
                    commencing: Some(monday),
                    sessions: week.sessions,
                    weeks,
                    reruns,
                    mesocycle,
                });
            };

            let placed = placed(&week.sessions);
            let state = planner::microcycle_state(&placed);
            if let Some(rerun) = planner::rerun(monday, state, &placed) {
                reruns.push(rerun);
            }
            weeks.push((monday, state));
            opens = Some(ended);
            monday = monday
                .checked_add(jiff::Span::new().weeks(1))
                .map_err(|_| StoreError::Corrupt {
                    detail: "a microcycle beyond the calendar".to_owned(),
                })?;
        }
    }

    /// Every session of the week commencing `monday`, with its state, against
    /// a plan already rescheduled — and whether it has ended by `now`.
    ///
    /// In the order the week runs. An empty week is one the diary says nothing
    /// about, which is a real state and not a fault; it ends with the calendar
    /// week, since nothing in it can end it sooner.
    ///
    /// `opens` is where the microcycle before it ended, and so where this one's
    /// record starts; `None` for the first week of the plan.
    async fn week_of(
        &self,
        plan: &Plan,
        diary: &Diary,
        monday: Date,
        opens: Option<Boundary>,
        now: DayPart,
    ) -> Result<Week, StoreError> {
        let week = planner::week(plan, diary, monday);
        if week.is_empty() {
            let next = monday
                .checked_add(jiff::Span::new().weeks(1))
                .map_err(|_| StoreError::Corrupt {
                    detail: "a microcycle beyond the calendar".to_owned(),
                })?;
            let next = DayPart::new(next, PartOfDay::Morning);
            return Ok(Week {
                sessions: Vec::new(),
                ended: (next <= now).then_some(Boundary::From(next)),
            });
        }

        let slots: Vec<ScheduledSlot> = week.iter().map(|planned| planned.slot).collect();

        // **What closes the last session's window comes from the diary**, not
        // from the end of the week: a Sunday session is still performable on
        // the Monday morning if nothing is slotted before then.
        let after = slots
            .last()
            .and_then(|last| diary.first_ordinary_after(last.date))
            .map(|slot| DayPart::new(slot.date, slot.slot.part));

        let recorded = self.performed(&week, opens, after, now.date).await?;
        let sessions_of = |upto: usize| -> Vec<RecordedSession> {
            recorded
                .iter()
                .take(upto)
                .map(|account| account.session)
                .collect()
        };
        let answered = accounted(&slots, &sessions_of(recorded.len()));

        // When each slot was first answered: the moment of the session that,
        // read in order, first made it performed. `None` where that session was
        // dated to a day and no finer, and the boundary below then falls back to
        // the slot's own start — which is where it already fell for a slot
        // nothing performed.
        let mut performed_at: Vec<Option<DateTime>> = vec![None; slots.len()];
        for (upto, account) in recorded.iter().enumerate() {
            let so_far = accounted(&slots, &sessions_of(upto.saturating_add(1)));
            for (at, done) in performed_at.iter_mut().zip(so_far) {
                if done && at.is_none() {
                    *at = account.at;
                }
            }
        }

        let mut sessions = Vec::with_capacity(week.len());
        let mut settled: Option<Boundary> = None;
        let mut outstanding = false;
        for (at, planned) in week.iter().enumerate() {
            let slot = planned.slot;
            let begins = DayPart::new(slot.date, slot.slot.part);
            let closes = slots
                .get(at.saturating_add(1))
                .map_or(after, |next| Some(DayPart::new(next.date, next.slot.part)));

            let recorded = Recorded {
                prescribed: self.prescribed(slot).await?,
                performed: answered.get(at).copied().unwrap_or(false),
            };
            let state = planner::state_of(begins, closes, now, diary, recorded);

            // **When it stopped being completable**: performed at the session
            // that performed it, skipped at its slot, and otherwise when its
            // window closed.
            let since = match state {
                SessionState::ToBePrescribed | SessionState::Prescribed => {
                    outstanding = true;
                    None
                }
                SessionState::Performed => Some(
                    performed_at
                        .get(at)
                        .copied()
                        .flatten()
                        .map_or(Boundary::From(begins), Boundary::After),
                ),
                SessionState::Skipped { .. } => Some(Boundary::From(begins)),
                SessionState::NotPrescribed | SessionState::NotPerformed { .. } => {
                    Some(Boundary::From(closes.unwrap_or(begins)))
                }
            };
            settled = match (settled, since) {
                (Some(settled), Some(since)) => Some(settled.later(since)),
                (settled, since) => settled.or(since),
            };

            sessions.push(Session {
                slot,
                number: planned.number,
                programmed: planned.session.is_ok(),
                test: planned.session.as_ref().is_ok_and(Filled::is_test),
                state,
            });
        }

        // **Ended once nothing in it can still be completed** (operator,
        // 2026-09-27): the last session that could be completed has been, or
        // its window has closed. Not merely once the final slot is answered —
        // a ride matched by class to the Sunday may be ridden on the Wednesday,
        // with the Wednesday's own session still owed.
        let ended = if outstanding { None } else { settled };

        Ok(Week { sessions, ended })
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

    /// What the record holds for this microcycle, when each session started,
    /// and what it says each was — oldest first.
    ///
    /// **From where the microcycle before it ended to where this one's final
    /// window closes**, and never past `today`: a session cannot have been
    /// performed in the future, and reading beyond the window would let next
    /// week's ride answer for this week's slot. The first week of a plan reads
    /// from its first slot's day.
    ///
    /// **The gym's sessions go in unnamed.** A Hevy workout names the day it
    /// was done; what session of the microcycle it *was* is only knowable where
    /// it was performed against a prescription, and reading that join is work
    /// nothing yet needs — the fallback in [`accounted`] gives the gym exactly
    /// the behaviour it had before (#177).
    async fn performed(
        &self,
        week: &[planner::Planned<'_>],
        opens: Option<Boundary>,
        closes: Option<DayPart>,
        today: Date,
    ) -> Result<Vec<Account>, StoreError> {
        let Some(first) = week.iter().map(|planned| planned.slot.date).min() else {
            return Ok(Vec::new());
        };
        let from = opens.map_or(first, Boundary::date);
        let to = closes.map_or(today, |closes| closes.date.min(today));
        if from > to {
            return Ok(Vec::new());
        }
        // A day-dated session is inside the closing boundary only where the whole
        // day is, for the reason [`Boundary::admits`] gives.
        let within = |on: Date, at: Option<DateTime>| {
            let before = |closes: DayPart| {
                at.map_or_else(|| on < closes.date, |at| DayPart::containing(at) < closes)
            };
            opens.is_none_or(|opens| opens.admits(on, at)) && closes.is_none_or(before)
        };

        let mut recorded: Vec<Account> = self
            .ports
            .gym_performed
            .performed_between(from, to)
            .await?
            .into_iter()
            .map(|occurred| {
                (
                    occurred.day(),
                    occurred.instant().map(|at| at.wall_clock().datetime()),
                )
            })
            .filter(|(on, at)| within(*on, *at))
            .map(|(on, at)| Account {
                on,
                at,
                session: RecordedSession::unnamed(on, Discipline::Gym),
            })
            .collect();

        for ridden in self
            .ports
            .cycling_performed
            .ridden_between(from, to)
            .await?
        {
            let on = ridden.started.date();
            if !within(on, Some(ridden.started)) {
                continue;
            }
            recorded.push(Account {
                on,
                at: Some(ridden.started),
                session: ridden_as(week, &ridden).map_or_else(
                    || RecordedSession::unnamed(on, Discipline::Cycling),
                    |role| RecordedSession::named(on, Discipline::Cycling, role),
                ),
            });
        }

        // Day first, then by the clock, a session with no clock first. One order
        // over two grains, and it is this reading's order rather than a fact
        // about either.
        recorded.sort_by_key(|account| (account.on, account.at));
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
    planner::rescheduled(plan, reruns, diary).map_err(|error: Unreschedulable| {
        StoreError::Corrupt {
            detail: format!("{} will not reschedule: {error}", plan.name()),
        }
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
