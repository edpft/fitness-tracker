//! Issuing the next prescribed workout.
//!
//! Generic over its ports, so the use case knows nothing about SQLite, TOML or
//! Hevy. The one direction § 11 permits runs inward: this reads the performed
//! record to derive the accessory slots, and nothing anywhere reads back.
//!
//! **The primary draws from programme state; every other slot draws from
//! observed history.** That split is the whole of what "primary" earns — a
//! warm-up ramp, a top set from the ladder, and back-offs — and it is decided by
//! asking the programme which slot is primary rather than by anything about the
//! exercise filling it.

use std::{borrow::Cow, cmp::Ordering, collections::BTreeMap};

use domain::{
    gym::{
        Load, SetKind,
        exercise::{DurationExercise, Exercise, RepsExercise},
    },
    measure::{Kg, RepCount},
    plan::{Occupies, Plan, PlanId, PlanName, Span},
    prescription::{
        Anchor, AnchorProvenance, Attempts, Block, BlockPeriodisation, BlockWeek, DerivedFrom,
        GatingTopSet, GenerationParameters, GymMesocycle, Linear, LoadSteps, Position,
        PrescribedExercise, PrescribedItem, PrescribedSet, PrescribedSuperset, PrescribedWorkout,
        PrescriptionState, Programming, Progress, Progression, Sbs, SbsDay, SbsSession, SlotId,
        SupersetMember, Target, Test, WeekKind, WeekPlan, WorkoutShape, anchor,
        linear::SlotContent,
        programming, progress_after, rep_max, rested,
        sbs::chart::{
            day as sbs_day, maximum_after as sbs_maximum_after, training_max_share, working_load,
        },
        warmup_ramp,
    },
    schedule::{Relative, SessionRole},
    sequence::{AtLeastTwo, NonEmpty},
};
use jiff::{Timestamp, civil::Date};

use crate::{
    error::{PrescriptionError, StoreError},
    ports::{
        Authored, ExerciseHistory, GenerationParameterStore, Issuance, LadderStanding,
        LastPerformance, MesocycleStore, Performance, PerformedSetSummary, PlanAuthor, PlanStore,
        PrescribedWorkoutStore, Prescription, PrescriptionLifecycle, UnderivableReason,
        UnderivableSlot, WorkoutPrescriber,
    },
};

/// What asking for the next session finds.
///
/// **Running out of plan is an answer, not a fault.** The operator, 2026-09-15,
/// on a date past the autumn's last day: *"there just isn't anything planned
/// after 2026-12-13."* So it is a variant beside the session rather than an
/// error, and a caller says it and exits cleanly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextSession {
    /// The first session at or after the date asked from.
    On(Date),
    /// Nothing is programmed at or after the date asked from.
    NothingPlanned {
        /// The plan whose gym mesocycles ran last, and the last day they
        /// occupy. `None` where no gym mesocycle has been authored at all.
        last: Option<(PlanName, Date)>,
    },
}

/// The first programmed session at or after a date.
///
/// **Capability, and it lives here because both driving adapters need the same
/// answer.** "The next session" is a statement about the operator's plan: it
/// reads the mesocycles and asks their calendars. It sat in `cli` until
/// 2026-08-30, which made a decision about training into something built into
/// a transport — a terminal and a browser could have disagreed about which
/// session was next, and nothing would have caught it.
///
/// **A function rather than a method on [`WorkoutPrescriber`]**, because the
/// only port it needs is the programme store. `deliver` and `compare` ask this
/// question too, and neither should have to construct a prescriber — a whole
/// generation apparatus, four ports deep — to find out what day it is asking
/// about.
///
/// **A starting point, not an exact match** (#122). It steps over days a block
/// skips, needs no mesocycle in force on the date, and crosses into the next
/// mesocycle when the one in force has nothing left — for the reason
/// [`crate::cycling::next_ride`] crosses: answering "nothing" there would be
/// the store's shape showing through as a gap in the plan. Answering for one
/// particular date is `prescribe`'s job, and it still refuses a day the block
/// does not run.
///
/// A date rather than an instant: which day *today* is belongs to the
/// operator's zone and a clock, and both are the caller's.
///
/// # Errors
///
/// [`StoreError`] if the store is unavailable or holds something unreadable.
/// Nothing about the plan is an error — see [`NextSession::NothingPlanned`].
pub async fn next_session(
    programmes: &(impl MesocycleStore + Sync),
    from: Date,
) -> Result<NextSession, StoreError> {
    // **The calendars answer, not this.** Which days a block runs, which weeks
    // it skips and where it ends are all their own; the only thing decided here
    // is which mesocycle to ask. The one in force answers first, and only one
    // with nothing left defers to those after it.
    let covering = programmes.on(from).await?;
    if let Some((_, _, mesocycle)) = &covering
        && let Some(date) = mesocycle.calendar().next_programmed(from)
    {
        return Ok(NextSession::On(date));
    }

    let mut after = from;
    while let Some((_, _, mesocycle)) = programmes.following(after).await? {
        let start = mesocycle.span().start();
        if let Some(date) = mesocycle.calendar().next_programmed(start) {
            return Ok(NextSession::On(date));
        }
        after = start;
    }

    // Where the plan ran out: the mesocycle in force if there is one — a date
    // after its last session is still inside it — else the last to finish
    // before the date.
    let last = match covering {
        Some(found) => Some(found),
        None => programmes.preceding(from).await?,
    };
    Ok(NextSession::NothingPlanned {
        last: last.map(|(_, plan, mesocycle)| {
            let end = mesocycle.span().end();
            (plan, end.yesterday().unwrap_or(end))
        }),
    })
}

/// Everything generation needs from the outside.
///
/// **`lifecycle` is here because a performed session is not re-derived.** The
/// use case cannot tell that from the prescription — a performed one and a
/// drafted one are the same row — so it asks, and asking is a port. It is the
/// same port `deliver` and the lifecycle report already use rather than a
/// second answer to one question.
pub struct PrescriptionPorts<H, P, G, S, L> {
    pub history: H,
    pub programmes: P,
    pub parameters: G,
    pub prescriptions: S,
    pub lifecycle: L,
}

/// The use case.
pub struct Prescribing<H, P, G, S, L> {
    ports: PrescriptionPorts<H, P, G, S, L>,
}

impl<H, P, G, S, L> Prescribing<H, P, G, S, L> {
    pub const fn new(ports: PrescriptionPorts<H, P, G, S, L>) -> Self {
        Self { ports }
    }
}

/// What one slot's derivation produced: an item, or a reason there is none.
///
/// The item is boxed because it is an order of magnitude larger than the reason,
/// and every slot returns one of these — an unboxed enum would size every result
/// to the larger arm.
enum Derived {
    Item(Box<PrescribedItem>),
    /// One or more slots the position could not deliver.
    ///
    /// More than one because a group is all-or-nothing: when one member of a
    /// supersetted pair or of the stretch circuit cannot be derived, the whole
    /// item is withheld, and every slot that went with it is owed a reason
    /// (FR-011). Reporting only the member that failed would leave the others
    /// missing from the session with nothing said about them.
    Underivable(Vec<UnderivableSlot>),
}

impl Derived {
    fn item(item: PrescribedItem) -> Self {
        Self::Item(Box::new(item))
    }

    fn underivable(slot: UnderivableSlot) -> Self {
        Self::Underivable(vec![slot])
    }
}

impl<H, P, G, S, L> WorkoutPrescriber for Prescribing<H, P, G, S, L>
where
    H: ExerciseHistory + Sync,
    P: MesocycleStore + Sync,
    G: GenerationParameterStore + Sync,
    S: PrescribedWorkoutStore + Sync,
    L: PrescriptionLifecycle + Sync,
{
    async fn standing(&self, on: Date) -> Result<LadderStanding, PrescriptionError> {
        let Some((programme_id, plan, programme)) = self.ports.programmes.on(on).await? else {
            return Err(PrescriptionError::NoPlan { date: on });
        };
        let Some((_, parameters)) = self.ports.parameters.current().await? else {
            return Err(PrescriptionError::NoParameters);
        };
        let programming = programming()?;
        let consulted = Consulted {
            parameters: &parameters,
            programming: &programming,
        };
        let progress = self.progress_of(&plan, &programme, &consulted, on).await?;
        let target = self.inheritance(&programme, &consulted, on).await?.target;
        let anchor = match programme.primary_exercise() {
            Exercise::Reps(primary) => self.anchor_in_force(primary, on, &consulted).await?,
            Exercise::Duration(_) | Exercise::Distance(_) => None,
        };
        Ok(LadderStanding {
            target,
            plan,
            programme_id,
            programme,
            parameters,
            anchor,
            progress,
            history_through: self.ports.history.newest_performance().await?,
        })
    }

    async fn prescribe(&self, date: Date) -> Result<Prescription, PrescriptionError> {
        // What is in force, and what has become of it. Read before any work,
        // because both answers decide what the derivation below is allowed to
        // do with its result.
        let in_force = self.ports.prescriptions.issued_for(date).await?;
        let state = match &in_force {
            Some((id, _)) => Some(self.ports.lifecycle.state_of(*id).await?),
            None => None,
        };

        // **A performed session stands, and is not derived at all.** Not
        // because superseding it would lose it — § 12 keeps every issue — but
        // because `compare` reads the prescription in force for a date, and
        // replacing it would leave the performance measured against a session
        // that was never trained. The record of what was prescribed is part of
        // what the performance means.
        //
        // Returning *before* the derivation rather than discarding one after it
        // is the point: a session that cannot be replaced is not a candidate
        // for replacement, and deriving one to throw away would imply it was.
        if let (Some((id, workout)), Some(PrescriptionState::Performed { reference })) =
            (&in_force, &state)
        {
            return Ok(Prescription {
                id: *id,
                workout: workout.clone(),
                issuance: Issuance::Performed {
                    reference: reference.clone(),
                },
                history_through: self.ports.history.newest_performance().await?,
                underivable: Vec::new(),
            });
        }

        let (workout, underivable) = self.derive(date).await?;

        // **The shape decides whether this is the same workout.** Not the whole
        // of `PrescribedWorkout`, which also carries when it was issued, which
        // programme version derived it and under which consulted — every one of
        // those a fact *about* the issuing rather than part of the session. A
        // superseded programme that produces the same exercises, in the same
        // order, with the same sets, reps and loads has produced the same
        // workout, and issuing it again would put a second session in front of
        // the operator to be delivered and trained.
        //
        // So an identical derivation writes nothing and the standing
        // prescription is returned as it stands — with its own identity, which
        // is what any delivery already made is recorded against.
        if let Some((previous, standing)) = in_force {
            if standing.shape() == workout.shape() {
                return Ok(Prescription {
                    id: previous,
                    workout: standing,
                    issuance: Issuance::Unchanged,
                    history_through: self.ports.history.newest_performance().await?,
                    underivable,
                });
            }

            let id = self.ports.prescriptions.issue(&workout).await?;
            let history_through = self.ports.history.newest_performance().await?;
            return Ok(Prescription {
                id,
                workout,
                issuance: Issuance::Superseded {
                    previous,
                    // A superseded prescription that had been delivered leaves
                    // that session at the destination until the next delivery
                    // replaces it in place (decision 0022). Reported rather
                    // than swallowed: between this command and `deliver` the
                    // operator's phone still holds the session they are no
                    // longer meant to train.
                    stranded: match state {
                        Some(PrescriptionState::Published { reference }) => Some(reference),
                        Some(PrescriptionState::Drafted | PrescriptionState::Performed { .. })
                        | None => None,
                    },
                },
                history_through,
                underivable,
            });
        }

        let id = self.ports.prescriptions.issue(&workout).await?;
        let history_through = self.ports.history.newest_performance().await?;

        Ok(Prescription {
            id,
            workout,
            issuance: Issuance::Issued,
            history_through,
            underivable,
        })
    }
}

impl<H, P, G, S, L> Prescribing<H, P, G, S, L>
where
    H: ExerciseHistory + Sync,
    P: MesocycleStore + Sync,
    G: GenerationParameterStore + Sync,
    S: PrescribedWorkoutStore + Sync,
    L: PrescriptionLifecycle + Sync,
{
    /// Derive the session the programme, the consulted and the record produce
    /// for a date.
    ///
    /// **Separate from `prescribe` because deriving and deciding what to do with
    /// the result are different questions.** This one is a pure function of the
    /// store's contents; the caller decides whether what comes out is worth
    /// writing down.
    async fn derive(
        &self,
        date: Date,
    ) -> Result<(PrescribedWorkout, Vec<UnderivableSlot>), PrescriptionError> {
        let Some((programme_id, plan, programme)) = self.ports.programmes.on(date).await? else {
            return Err(PrescriptionError::NoPlan { date });
        };
        let Some((parameters_at, parameters)) = self.ports.parameters.current().await? else {
            return Err(PrescriptionError::NoParameters);
        };
        let programming = programming()?;
        let consulted = Consulted {
            parameters: &parameters,
            programming: &programming,
        };

        let (week, role) = programme.calendar().place(date)?;

        // Every exercise the programme can prescribe that progresses, in one
        // call. Both sides of every alternating fill, because this session
        // prescribes one and the next needs the other's history — and only the
        // repetitions vocabulary, because a hold does not progress and the port
        // will not be asked about one.
        let wanted: Vec<RepsExercise> = programme
            .fills()
            .every_exercise()
            .into_iter()
            .filter_map(|exercise| match exercise {
                Exercise::Reps(reps) => Some(reps),
                Exercise::Duration(_) | Exercise::Distance(_) => None,
            })
            .collect();
        let history = self.ports.history.last_performances(&wanted).await?;

        // **Where the primary's rung comes from.** The calendar says whether this
        // is a climbing week or the test; it does not say which rung, because a
        // miss holds the ladder and a stall suspends it. So the position is walked
        // out of the gating sessions performed so far (US3) and is derived on every
        // read — there is no stored counter to advance twice.
        let progress = self
            .progress_of(&plan, &programme, &consulted, date)
            .await?;

        // What a test week takes from the programme before it: the target it is
        // an attempt at, and the load its other session runs at. Both are empty
        // for a programme that climbs, which has neither question to ask.
        let inheritance = self.inheritance(&programme, &consulted, date).await?;

        // **The derivation gets the calendar's week and the record gets the
        // programme's.** `Calendar::place` reports every week as a climbing one
        // since decision 0013 — which of them is a test is the block's business,
        // decided by its phase plan and by whether it measures its own entry. So
        // the index the derivation needs survives, and what is stored is what
        // the week actually was.
        let recorded = week_of(&programme, week);

        // **Resolved once, and used for both the loads and the record.** A
        // cycle that inherits has no authored number, so what it opens from is
        // the predecessor's measurement — and that is what the prescription
        // records it as descending from.
        let opening = match programme.primary_exercise() {
            Exercise::Reps(primary) => self.anchor_in_force(primary, date, &consulted).await?,
            Exercise::Duration(_) | Exercise::Distance(_) => None,
        };

        let mut items = Vec::new();
        let mut underivable = Vec::new();
        let standing = Standing {
            progress,
            inheritance,
            maximum: self.maximum_of(&plan, &programme, &consulted, date).await?,
            opening,
        };
        for derived in issue_slots(&programme, &consulted, role, week, standing, &history) {
            match derived {
                Derived::Item(item) => items.push(*item),
                Derived::Underivable(slots) => underivable.extend(slots),
            }
        }

        let items = NonEmpty::new(items).map_err(|_| PrescriptionError::NothingDerivable)?;

        // **Rest is filled in over the assembled session, not slot by slot.**
        // What a set rests for depends on which block it is in and on whether
        // another member of its item follows it, and the second of those is not
        // known while a slot is still being derived — the grouping happens
        // above.
        let shape = rested(&WorkoutShape::new(items), &consulted.parameters.rest);

        let workout = PrescribedWorkout::new(
            shape,
            date,
            role,
            recorded,
            derived_from(&plan, &programme, inheritance, opening)?,
            parameters,
            parameters_at,
            programme_id,
            Timestamp::now(),
        );

        Ok((workout, underivable))
    }

    /// Where the primary's progression stands, walked out of the record.
    ///
    /// **Only the gating role gates** (US3-10). A miss on the other session says
    /// nothing about the ladder, so the other session's sets never reach the
    /// mechanism — which is the filter below and not a rule inside it.
    ///
    /// **Only sessions inside this block count.** A date the calendar will not
    /// place is before the block, after it, or in a week it skips, and none of
    /// those is a rung of this plan.
    /// **Only sessions before the date being prescribed.** A prescription is
    /// issued before the session it prescribes, so a session on the day itself is
    /// not evidence about what to do that day — and issuing for a past date would
    /// otherwise read forward through the record and answer with a rung the
    /// operator could not have been given at the time.
    /// Where the record puts a programme, for the one template that has a
    /// position to be at.
    ///
    /// **`None` is not "at the start".** A block's loads are shares of its
    /// anchor and a test has no ladder at all, so neither has a rung a miss
    /// could hold — and reporting them as climbing week one would be a number
    /// with no meaning behind it rather than an absence.
    async fn progress_of(
        &self,
        plan: &PlanName,
        programme: &GymMesocycle,
        consulted: &Consulted<'_>,
        before: Date,
    ) -> Result<Option<Progress>, PrescriptionError> {
        match programme {
            GymMesocycle::Progression(Progression::Linear(linear)) => {
                Ok(Some(self.progress(plan, linear, consulted, before).await?))
            }
            // **Neither has a rung.** A block's loads are shares of a fixed
            // anchor; an SBS cycle's are shares of a maximum that moves, but it
            // moves off measured results rather than off a ladder position, so
            // there is still nothing here for a miss to hold.
            GymMesocycle::Progression(
                Progression::BlockPeriodisation(_) | Progression::Provided { .. },
            )
            | GymMesocycle::Test(_) => Ok(None),
        }
    }

    /// The maximum this programme is a share of, where that moves with the
    /// record.
    ///
    /// `None` for every template but SBS: a ladder's anchor is fixed and a
    /// block's is too, so there is nothing here for them to answer.
    async fn maximum_of(
        &self,
        plan: &PlanName,
        programme: &GymMesocycle,
        consulted: &Consulted<'_>,
        before: Date,
    ) -> Result<Option<Kg>, PrescriptionError> {
        match programme {
            GymMesocycle::Progression(Progression::Provided { cycle: sbs, .. }) => {
                Ok(Some(self.sbs_maximum(plan, sbs, consulted, before).await?))
            }
            GymMesocycle::Progression(
                Progression::Linear(_) | Progression::BlockPeriodisation(_),
            )
            | GymMesocycle::Test(_) => Ok(None),
        }
    }

    /// The maximum an SBS cycle is programming from, on a given date.
    ///
    /// **The chart's percentages are shares of a number that moves**, and it
    /// moves off performance: each week's second session finds a repetition
    /// maximum, and SBS's own table turns that into what the next week
    /// programmes from. So this walks the cycle's own gating sessions in order
    /// and advances the opening anchor through each one.
    ///
    /// **Loads depending on performance is the expected behaviour**, not a
    /// complication — a linear ladder reads the record for the same reason. What
    /// differs is what is read: a ladder asks whether the top set was completed,
    /// and this asks what it weighed.
    ///
    /// A session that recorded no working set advances nothing, which is right:
    /// a week nobody trained leaves the maximum where it was.
    /// What this session's loads are shares of.
    ///
    /// **Asked as at the session's own date, because an anchor belongs to a
    /// microcycle.** It changes week to week: a week that measures something
    /// leaves a new one behind and the week after it programmes from that. A
    /// number resolved once for a whole mesocycle could only ever describe its
    /// first week, which is the shape this replaced.
    ///
    /// Three places one can come from, and the latest at or before the date
    /// wins:
    ///
    /// ```text
    /// a mesocycle that finished    what the record shows it measured
    /// this mesocycle's entry test  what the record shows that week measured
    /// a test at the front          the number it asserts, because nothing
    ///                              before it measured one
    /// ```
    ///
    /// **Only the third is authored**, and its provenance says so. The other two
    /// are read off the record, which is what lets the same authored cycle be
    /// run again in January against January's record.
    ///
    /// `None` for a lift nothing has measured and no test has asserted.
    async fn anchor_in_force(
        &self,
        primary: RepsExercise,
        on: Date,
        consulted: &Consulted<'_>,
    ) -> Result<Option<Anchor>, PrescriptionError> {
        let mut candidates = Vec::new();

        if let Some((_, _, current)) = self.ports.programmes.on(on).await? {
            // A week at the front of a sequence states what it ramps toward,
            // whether it stands alone or is the first week of a block.
            if let Some(asserted) = asserted_by(&current) {
                candidates.push(asserted);
            }
            if let Some(measured) = self.measured_entering(&current, primary, on).await? {
                candidates.push(measured);
            }
        }

        if let Some((_, before_plan, before)) = self.ports.programmes.preceding(on).await? {
            if let Some(asserted) = asserted_by(&before) {
                candidates.push(asserted);
            }
            if let Some(measured) = self.left_behind(&before_plan, &before, consulted).await? {
                candidates.push(measured);
            }
        }

        Ok(anchor::in_force(&candidates, on))
    }

    /// What this mesocycle's own entry test measured, where it has one and it
    /// has already run.
    ///
    /// **A block measures its own entry and then programmes from it.** Before
    /// this, the block was authored with a number saying what the operator
    /// expected and the test week only confirmed it — a result that differed was
    /// answered by re-authoring the block. The week is the microcycle that
    /// produces the anchor the rest of the block is shares of, so it is read
    /// like any other measurement.
    ///
    /// **Only the entry week's sessions count.** Every other week of the block
    /// lifts heavy without measuring anything, and the heaviest completed set of
    /// an ordinary week is a working set rather than a maximum — so the
    /// calendar is asked which week each performance fell in rather than the
    /// span being taken whole.
    async fn measured_entering(
        &self,
        programme: &GymMesocycle,
        primary: RepsExercise,
        before: Date,
    ) -> Result<Option<Anchor>, PrescriptionError> {
        let GymMesocycle::Progression(Progression::BlockPeriodisation(block)) = programme else {
            return Ok(None);
        };
        if block.entry_test().is_none() {
            return Ok(None);
        }
        let calendar = block.calendar();
        let performances = self.ports.history.performances(primary).await?;
        let entering: Vec<_> = performances
            .into_iter()
            .filter(|performance| performance.on < before)
            .filter(|performance| {
                matches!(
                    calendar.place(performance.on),
                    Ok((WeekKind::Climbing(week), _)) if week == domain::prescription::WeekIndex::FIRST
                )
            })
            .collect();
        let Some(latest) = entering.iter().map(|performance| performance.on).max() else {
            return Ok(None);
        };
        let Some(completed) = heaviest_completed_in(&entering) else {
            return Ok(None);
        };
        let failed = entering
            .iter()
            .flat_map(|performance| performance.sets.iter())
            .filter_map(|set| match (set.load, set.outcome.completed()) {
                (Load::Absolute(mass), None) if mass > completed => Some(mass),
                _ => None,
            })
            .max();
        Ok(Anchor::new(completed, failed, AnchorProvenance::Tested, latest).ok())
    }

    /// The maximum a mesocycle actually measured, as the record has it.
    ///
    /// **Two things measure one** ([`GymMesocycle::produces_maximum`]): a test week,
    /// which is the whole of what it is for, and a provided cycle, whose last
    /// session is a one-repetition maximum. A ladder measures nothing even when
    /// its last single felt like a test.
    ///
    /// A provided predecessor is asked through [`Self::sbs_maximum`] rather than
    /// read directly, so the chart converts what was lifted into what the next
    /// cycle programmes from — the same arithmetic that moves the maximum
    /// *inside* a cycle, applied across the join. Boxed because that call comes
    /// back here for its own opening, and a chain of three cycles is three hops.
    fn left_behind<'a>(
        &'a self,
        plan: &'a PlanName,
        mesocycle: &'a GymMesocycle,
        consulted: &'a Consulted<'a>,
    ) -> std::pin::Pin<
        Box<dyn Future<Output = Result<Option<Anchor>, PrescriptionError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let span = mesocycle.span();
            let measured = match mesocycle {
                GymMesocycle::Progression(Progression::Provided { cycle: before, .. }) => {
                    // Asked the day after it ends, so its own last session — the
                    // one-repetition maximum — counts toward what it leaves.
                    let after = span.end().tomorrow().unwrap_or_else(|_| span.end());
                    Some((
                        self.sbs_maximum(plan, before, consulted, after).await?,
                        None,
                    ))
                }
                GymMesocycle::Test(test) => match test.primary_exercise() {
                    Exercise::Reps(primary) => self.measured_in(primary, span).await?,
                    Exercise::Duration(_) | Exercise::Distance(_) => None,
                },
                GymMesocycle::Progression(
                    Progression::Linear(_) | Progression::BlockPeriodisation(_),
                ) => None,
            };

            Ok(measured.and_then(|(load, failed)| {
                Anchor::new(load, failed, AnchorProvenance::Tested, span.end()).ok()
            }))
        })
    }

    /// What the record says a lift measured inside a span: the heaviest single
    /// that went up, and the heaviest that did not above it.
    ///
    /// **Both halves, because a block's opening is derived from the second.** A
    /// test that found the ceiling completed one load and failed the one above
    /// it; the completed load is the maximum and the failed one is what the
    /// opening drops off. Reading only the first would silently move every
    /// derived opening from "the failed load, dropped" to "one climb above what
    /// went up", which is the same rule's other branch and a different number.
    async fn measured_in(
        &self,
        primary: RepsExercise,
        span: Span,
    ) -> Result<Option<(Kg, Option<Kg>)>, PrescriptionError> {
        let performances = self.ports.history.performances(primary).await?;
        let Some(completed) = heaviest_completed(&performances, span) else {
            return Ok(None);
        };
        Ok(Some((
            completed,
            heaviest_failed_above(&performances, span, completed),
        )))
    }

    async fn sbs_maximum(
        &self,
        plan: &PlanName,
        sbs: &Sbs,
        consulted: &Consulted<'_>,
        before: Date,
    ) -> Result<Kg, PrescriptionError> {
        let Exercise::Reps(primary) = sbs.primary_exercise() else {
            return Err(PrescriptionError::NoInheritedMaximum {
                start: sbs.calendar().start(),
            });
        };
        let Some(opening) = self
            .anchor_in_force(primary, sbs.calendar().start(), consulted)
            .await?
        else {
            return Err(PrescriptionError::NoInheritedMaximum {
                start: sbs.calendar().start(),
            });
        };
        let mut maximum = opening.load();
        // The increment is the plate grid's, not a number of this module's:
        // what SBS's `FLOOR` rounds to is whatever the bar can actually hold.
        let Some(steps) = consulted
            .parameters
            .scales
            .for_exercise(Exercise::Reps(primary))
        else {
            return Ok(maximum);
        };
        let increment = steps.step_at(maximum);

        let mut performances = self.ports.history.performances(primary).await?;
        // In date order, because each advance is applied to the result of the
        // one before it. The store's order is not part of its contract.
        performances.sort_by_key(|performance| performance.on);

        // Gathered first and applied by the chart, because *what* the record
        // says is this layer's business and what it *means* is the chart's.
        let mut performed: Vec<(u32, Kg)> = Vec::new();
        for performance in &performances {
            if performance.on >= before {
                continue;
            }
            // Same two answers the ladder uses, and in the same order: the
            // prescription where the record links one, the calendar otherwise.
            let (week, role) = match &performance.fulfilled {
                Some(fulfilled) if fulfilled.plan == *plan => {
                    match sbs.calendar().place(performance.on) {
                        Ok((week, _)) => (week, fulfilled.role),
                        Err(_) => continue,
                    }
                }
                _ => match sbs.calendar().place(performance.on) {
                    Ok(placed) => placed,
                    Err(_) => continue,
                },
            };
            if role != sbs.gating_role() {
                continue;
            }
            let WeekKind::Climbing(index) = week else {
                continue;
            };
            let Some(top) = top_set_of(performance) else {
                continue;
            };
            // A failed attempt says what the operator could not do, which is not
            // a repetition maximum and advances nothing.
            if !top.completed {
                continue;
            }
            performed.push((index.as_u32(), top.load));
        }

        maximum = sbs_maximum_after(maximum, &performed, increment);
        Ok(maximum)
    }

    /// What a test week takes from the programme before it (decision 0013).
    ///
    /// Two questions of one predecessor, so it is read once. The target is what
    /// the heavy session is an attempt at, and the light load is what the other
    /// session runs the predecessor's primary at.
    ///
    /// **The target is refused across a change of lift.** A front squat maximum
    /// is not evidence about an RDL, so a predecessor training a different lift
    /// answers the first question with nothing, and the maximum in force for the
    /// lift being tested answers it instead. It still answers the second: the
    /// light session is the predecessor's session whatever it was training.
    async fn inheritance(
        &self,
        programme: &GymMesocycle,
        consulted: &Consulted<'_>,
        date: Date,
    ) -> Result<Inheritance, PrescriptionError> {
        let GymMesocycle::Test(test) = programme else {
            return Ok(Inheritance {
                target: None,
                light: None,
            });
        };

        // **What the lift is at, wherever that comes from.** A test used to be
        // able to state its target outright, for the case where there was
        // nothing before it to inherit from; that is now a stated measurement
        // for the lift, read here like any other.
        let stated = match test.primary_exercise() {
            Exercise::Reps(primary) => self
                .anchor_in_force(primary, test.calendar().start(), consulted)
                .await?
                .map(Anchor::load),
            Exercise::Duration(_) | Exercise::Distance(_) => None,
        };

        let predecessor = self
            .ports
            .programmes
            .preceding(test.calendar().start())
            .await?;
        let Some((_, before_plan, GymMesocycle::Progression(Progression::Linear(before)))) =
            predecessor
        else {
            // Nothing before it, or a predecessor with no ladder to read a
            // position off. A block's exit test anchors what follows through its
            // own result rather than through a target, so a test after one has
            // nothing to inherit either.
            return Ok(Inheritance {
                target: stated,
                light: None,
            });
        };

        let progress = self
            .progress(&before_plan, &before, consulted, date)
            .await?;
        let before_primary = match before.primary_exercise() {
            Exercise::Reps(primary) => Some(primary),
            Exercise::Duration(_) | Exercise::Distance(_) => None,
        };
        let before_opening = match before_primary {
            Some(primary) => {
                self.anchor_in_force(primary, before.calendar().start(), consulted)
                    .await?
            }
            None => None,
        };
        let Some(before_opening) = before_opening else {
            return Ok(Inheritance {
                target: stated,
                light: None,
            });
        };
        let Ok(ladder) = before.ladder(before_opening, consulted.parameters) else {
            return Ok(Inheritance {
                target: stated,
                light: None,
            });
        };
        let Ok(steps) = before.steps(consulted.parameters) else {
            return Ok(Inheritance {
                target: stated,
                light: None,
            });
        };

        let inherited = (before.primary_exercise() == test.primary_exercise())
            .then(|| progress.test_target(ladder, steps));
        Ok(Inheritance {
            // **An asserted number wins**, and it is the operator saying what
            // this week is for. Reading one is the default; a week that states
            // one states it because nothing behind it measured the lift, and a
            // ladder's position is not a measurement — the predecessor never
            // tested, which is exactly why the number was asserted.
            target: stated.or(inherited),
            light: progress.light_top_set(ladder, steps, consulted.parameters.light_of_heavy),
        })
    }

    async fn progress(
        &self,
        plan: &PlanName,
        programme: &Linear,
        consulted: &Consulted<'_>,
        before: Date,
    ) -> Result<Progress, PrescriptionError> {
        let Exercise::Reps(primary) = programme.primary_exercise() else {
            // A programme whose primary is not counted in repetitions has no
            // ladder to be at a position on. Authoring refuses one (A-5), so this
            // is the type system's edge rather than a state to handle.
            return Ok(Progress::Climbing {
                week: domain::prescription::WeekIndex::FIRST,
            });
        };

        let performances = self.ports.history.performances(primary).await?;
        let mut gating: Vec<GatingTopSet> = Vec::new();
        for performance in &performances {
            if performance.on >= before {
                continue;
            }
            // **The prescription answers where it can; the calendar answers
            // otherwise.**
            //
            // A published id is the only thing that *links* a performance to a
            // prescription, and where there is one it is the better answer: it
            // records what the session was rather than inferring it from when it
            // happened. That is what makes the heavy session prescribed for
            // Friday and performed on Saturday morning gate -- `place` refuses
            // the Saturday, and the operator's rule is that the performance is
            // the fact.
            //
            // But the sessions of a block trained before any of this existed
            // were still trained, and a record that cannot say which session it
            // was is not a record of nothing. So the calendar keeps answering
            // for them, exactly as it did. It is the weaker answer -- an
            // unlinked session performed a day late is still dropped, and
            // nothing here can recover it -- and it is the only one available.
            //
            // 0018 removes the fallback by removing the calendar. Until then a
            // programme that predates the link keeps its ladder position.
            let role = match &performance.fulfilled {
                Some(fulfilled) if fulfilled.plan == *plan => fulfilled.role,
                _ => match programme.calendar().place(performance.on) {
                    Ok((_, role)) => role,
                    Err(_) => continue,
                },
            };
            if role != programme.gating_role() {
                continue;
            }
            if let Some(top) = top_set_of(performance) {
                gating.push(top);
            }
        }

        Ok(progress_after(
            &gating,
            consulted.programming.first_reset,
            consulted.programming.second_reset,
            programme.steps(consulted.parameters)?,
        ))
    }
}

/// The heaviest completed set of any session in a span.
///
/// **Every set of every session, warm-ups included** (issue #127). Two
/// narrowings used to stand between this question and the record, and the case
/// that matters defeats both: a test week ramps toward an attempt and misses it.
/// The store excluded warm-ups, so a ramp's completed singles were invisible;
/// and asking [`top_set_of`] collapsed each session to its heaviest set *before*
/// asking whether it completed, so a session whose heaviest set was the failure
/// answered nothing at all — leaving the light session's taper as the only
/// candidate, or nothing.
///
/// A failed attempt says what could not be lifted, which is not a maximum. What
/// was lifted is the heaviest thing that went up, wherever in the session it
/// sits and whatever the source tagged it.
/// The heaviest set of a span that did *not* go up, above a load that did.
///
/// **A ceiling, not a failure.** What a derived opening drops off is the load a
/// test proved was too much, so a failed attempt lighter than the heaviest
/// completed single says nothing — it is a bad day inside the ramp rather than
/// the top of the range.
/// The anchor a mesocycle's opening entry test states, where it states one.
///
/// **Two shapes of the same week.** A standalone test is a mesocycle of its own;
/// a block that measures its own entry carries the week in front of its phases.
/// Either is the first microcycle of a sequence when nothing before it measured
/// the lift, and either may therefore assert.
fn asserted_by(programme: &GymMesocycle) -> Option<Anchor> {
    match programme {
        GymMesocycle::Test(test) => test.asserted(),
        GymMesocycle::Progression(Progression::BlockPeriodisation(block)) => block
            .entry_test()
            .and_then(domain::prescription::EntryTest::asserted),
        GymMesocycle::Progression(Progression::Linear(_) | Progression::Provided { .. }) => None,
    }
}

fn heaviest_failed_above(performances: &[Performance], span: Span, completed: Kg) -> Option<Kg> {
    performances
        .iter()
        .filter(|performance| span.covers(performance.on))
        .flat_map(|performance| performance.sets.iter())
        .filter_map(|set| match (set.load, set.outcome.completed()) {
            (Load::Absolute(mass), None) if mass > completed => Some(mass),
            _ => None,
        })
        .max()
}

fn heaviest_completed_in(performances: &[Performance]) -> Option<Kg> {
    performances
        .iter()
        .flat_map(|performance| performance.sets.iter())
        .filter_map(|set| match (set.load, set.outcome.completed()) {
            (Load::Absolute(mass), Some(_)) => Some(mass),
            _ => None,
        })
        .max()
}

fn heaviest_completed(performances: &[Performance], span: Span) -> Option<Kg> {
    performances
        .iter()
        .filter(|performance| span.covers(performance.on))
        .flat_map(|performance| performance.sets.iter())
        .filter_map(|set| match (set.load, set.outcome.completed()) {
            // Only an absolute load is comparable on this axis, as in the gate
            // below: an assisted or weighted bodyweight lift is not measured
            // from a mass a maximum could be taken of.
            (Load::Absolute(mass), Some(_)) => Some(mass),
            _ => None,
        })
        .max()
}

/// A gating session's top set: the heaviest set it holds, and what became of it.
///
/// **Heaviest rather than first.** In a session issued from this template the top
/// set is the first working set and the back-offs are lighter, so the two agree;
/// in the hand-run record they do not always, because that block opened with heavy
/// bridging singles tagged as warm-ups. Taking the heaviest is right under both
/// readings, and the failed attempt this exists to notice is by construction the
/// heaviest thing attempted.
///
/// **Warm-ups are among the candidates** (issue #127), which is what makes that
/// second reading true rather than merely intended: the bridging singles the
/// paragraph above describes were filtered out by the store until then. Both
/// mechanisms reading this respond to what was lifted — the ladder steps from
/// the completed load and SBS back-computes its maximum from it — so a load
/// lower than the one prescribed regulates them rather than misleading them.
///
/// `None` where the session recorded no set on this axis at all, which is a
/// session that says nothing about the ladder rather than a miss.
fn top_set_of(performance: &Performance) -> Option<GatingTopSet> {
    let mut heaviest: Option<(u64, &PerformedSetSummary)> = None;
    for set in &performance.sets {
        // Only an absolute load is comparable on this axis, and the primary is a
        // barbell lift. A relative one — assisted or weighted bodyweight — is left
        // out rather than compared against a mass it is not measured from.
        let Load::Absolute(mass) = set.load else {
            continue;
        };
        let grams = mass.as_grams();
        if heaviest.is_none_or(|(held, _)| grams > held) {
            heaviest = Some((grams, set));
        }
    }
    heaviest.map(|(_, set)| GatingTopSet {
        load: match set.load {
            Load::Absolute(mass) => mass,
            Load::Relative(_) => Kg::NONE,
        },
        completed: set.outcome.completed().is_some(),
    })
}

/// What a derivation consults, and it is two kinds of thing.
///
/// **The consulted are facts about the world the training happens in** — the
/// load steps each implement moves in, what the week allows — and they are
/// authored, dated and read from the store. **The programming is the shape the
/// operator trains**: the ramp, the back-offs, the top sets, the accessory
/// schemes and the reset protocols. It has one value, it is not authored, and
/// it is fixed in this build.
///
/// They travel together for the reason [`Standing`] does: every derivation
/// below needs both, and `issue_slots` was already at the argument limit
/// carrying one of them.
#[derive(Debug, Clone, Copy)]
struct Consulted<'a> {
    parameters: &'a GenerationParameters,
    programming: &'a Programming,
}

/// What the record says, gathered before any slot is derived.
///
/// **Three answers to one question — where does this session stand?** They
/// travel together because every one of them is read from the record before the
/// derivation begins, and because passing them separately had `issue_slots` at
/// the argument limit with a fourth already needed.
#[derive(Debug, Clone, Copy)]
struct Standing {
    /// Where a ladder has got to. `None` for every template without one.
    progress: Option<Progress>,
    /// What a test week takes from the programme before it.
    inheritance: Inheritance,
    /// The maximum an SBS cycle is currently a share of, advanced through the
    /// chart's own table by each rep-max day already performed. `None` for every
    /// template whose maximum does not move inside the mesocycle.
    maximum: Option<Kg>,
    /// What this mesocycle's loads are shares of, as the record and the stated
    /// series had it when it opened. `None` for a test, and for a lift nothing
    /// has measured.
    opening: Option<Anchor>,
}

/// Every position the template issues, derived in order.
///
/// The order is [`PrimaryPattern::sequence`]'s; all this adds is which
/// derivation each position gets — the primary its top set and back-offs, and
/// everything else double progression, a hold, or its authored numbers.
fn issue_slots(
    programme: &GymMesocycle,
    consulted: &Consulted<'_>,
    role: SessionRole,
    week: WeekKind,
    standing: Standing,
    history: &BTreeMap<RepsExercise, LastPerformance>,
) -> Vec<Derived> {
    let primary = programme.primary();
    primary
        .sequence()
        .into_iter()
        .map(|position| match position {
            Position::Single(slot) if slot == primary.slot() => {
                primary_slot_item(programme, consulted, role, week, standing)
            }
            Position::Single(slot) => accessory_slot(programme, consulted, role, history, slot),
            Position::Superset(first, second) => {
                group(programme, consulted, role, history, first, second, &[])
            }
            Position::Circuit([first, second, third, fourth]) => group(
                programme,
                consulted,
                role,
                history,
                first,
                second,
                &[third, fourth],
            ),
        })
        .collect()
}

/// The session a block's entry test is taken on.
///
/// Heavy, and for the same reason a standalone test's is: it is the week's whole
/// purpose, and the other session is whatever the block states for it.
const BLOCK_ENTRY_TEST_ROLE: SessionRole = SessionRole::new(Relative::Higher, Relative::Lower);

/// Which week this session belongs to, in the vocabulary the store speaks.
///
/// **The calendar cannot answer for a block.** Since decision 0013 a calendar
/// emits nothing but climbing weeks — a linear programme has no test and a
/// block's entry test is not one of its weeks — so which week is a block's exit
/// test is decided by the phase plan and nothing else. A standalone test week is
/// a test week on both its sessions: the week is what it is, and which session
/// is the attempt is the role's business.
fn week_of(programme: &GymMesocycle, placed: WeekKind) -> WeekKind {
    match programme {
        GymMesocycle::Test(_) => WeekKind::Test,
        // A linear programme's weeks are all climbing weeks, and an SBS cycle's
        // are too: **week 4 is not a test week even though it ends on a test**,
        // because its first session is a taper the chart states in full. Calling
        // the week a test would send the light session looking for a predecessor
        // to inherit from, which an SBS cycle never needs.
        GymMesocycle::Progression(Progression::Linear(_) | Progression::Provided { .. }) => placed,
        GymMesocycle::Progression(Progression::BlockPeriodisation(block)) => {
            let WeekKind::Climbing(index) = placed else {
                return placed;
            };
            block.kind(index).unwrap_or(placed)
        }
    }
}

/// What this session's primary loads were derived from, recorded by value.
///
/// # Errors
///
/// [`PrescriptionError::NoTarget`] for a test whose target is inherited and
/// whose predecessor cannot supply one. A test week with no target is not a
/// session with one slot missing — it is a week whose whole purpose is
/// unanswerable, so it is refused rather than issued incomplete.
fn derived_from(
    plan: &PlanName,
    programme: &GymMesocycle,
    inheritance: Inheritance,
    opening: Option<Anchor>,
) -> Result<DerivedFrom, PrescriptionError> {
    match programme {
        // **Resolved, because there is nothing else it could be.** No programme
        // states a maximum any more: every one of them reads the record and the
        // stated series for the lift it trains, and what comes back is recorded
        // here by value so a session issued in November says what it descended
        // from. Absent, the session is refused rather than issued against
        // nothing.
        GymMesocycle::Progression(_) => {
            opening
                .map(DerivedFrom::Anchor)
                .ok_or_else(|| PrescriptionError::NoInheritedMaximum {
                    start: programme.calendar().start(),
                })
        }
        GymMesocycle::Test(test) => {
            inheritance
                .target
                .map(DerivedFrom::Target)
                .ok_or_else(|| PrescriptionError::NoTarget {
                    plan: plan.clone(),
                    start: test.calendar().start(),
                })
        }
    }
}

/// What this session's primary slot is loaded from.
///
/// **Computed once, before any slot is derived.** The three templates answer the
/// question in three different places — a linear rung comes from the record, a
/// block's week from its own phase plan, and a test's from the programme before
/// it — and only the last of those needs another programme read out of the
/// store. Resolving it up front keeps the derivation below synchronous and keeps
/// the store read out of a loop over seventeen slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Inheritance {
    /// What a standalone test is an attempt at (decision 0011).
    target: Option<Kg>,
    /// What the light session of a test week runs its primary at: the load the
    /// predecessor's progression stands at, which is what makes the week the
    /// predecessor's session and a test rather than two tests.
    light: Option<Kg>,
}

/// How the primary slot's sets are built, once a load is known.
#[derive(Clone, Copy)]
enum PrimaryLoad {
    /// A ramp, one top set, and the role's back-offs. The linear rung, and the
    /// light session of a test week, which is a linear session by inheritance.
    TopSet { load: Kg, reps: RepCount },
    /// A ramp and then sets across, all at one load. Block accumulation, where
    /// no set can be a maximum because there are five of them.
    Across {
        load: Kg,
        sets: RepCount,
        reps: RepCount,
    },
    /// A ramp, then three attempts a step apart, the last at `toward` (#136).
    ///
    /// **Nothing is prescribed past the target**, because the target is a
    /// guess: the operator picks a number to attempt, and anything lifted beyond
    /// it is the record's to take rather than the plan's to ask for. The ramp
    /// leads into the first attempt; see [`Attempts`].
    Attempt { toward: Kg, reps: RepCount },
    /// A ramp, one attempt at a repetition maximum, then back-offs **at
    /// whatever that turned out to be**.
    ///
    /// **The back-offs carry no load, and cannot.** SBS's chart says `3 × 5–6 @
    /// 8RM`: the load is the eight-rep maximum found in the top set of the same
    /// session, so it does not exist when the prescription is issued. Writing a
    /// number there would mean predicting the attempt and then prescribing
    /// against the prediction.
    ///
    /// `toward` is what the ramp builds to and what the operator should expect
    /// to meet — the current maximum taken through SBS's own table at this
    /// repetition count, which is the exact inverse of the advance the result
    /// will produce. Derived, not guessed.
    RepMax {
        toward: Kg,
        reps: RepCount,
        back_off_sets: RepCount,
        back_off_reps: Target<RepCount>,
    },
}

/// The primary slot: a warm-up ramp, and then whatever this template asks for.
fn primary_slot_item(
    programme: &GymMesocycle,
    consulted: &Consulted<'_>,
    role: SessionRole,
    week: WeekKind,
    standing: Standing,
) -> Derived {
    let Standing {
        progress: _,
        inheritance,
        maximum,
        opening: _,
    } = standing;
    let pattern = programme.primary();
    let slot = pattern.slot();
    let exercise = programme.fills().primary(pattern, role);
    let Exercise::Reps(reps_exercise) = exercise else {
        return Derived::underivable(UnderivableSlot {
            slot,
            exercise: exercise.as_str(),
            reason: UnderivableReason::NotCountedInReps,
        });
    };

    let Some(steps) = consulted.parameters.scales.for_exercise(*exercise) else {
        return Derived::underivable(UnderivableSlot {
            slot,
            exercise: exercise.as_str(),
            reason: UnderivableReason::NoLoadScale,
        });
    };

    let plan = match programme {
        GymMesocycle::Progression(Progression::Linear(linear)) => {
            linear_load(linear, consulted, role, week, standing, steps)
        }
        GymMesocycle::Progression(Progression::BlockPeriodisation(block)) => {
            block_load(block, role, week, standing.opening, steps)
        }
        GymMesocycle::Progression(Progression::Provided { cycle: sbs, .. }) => {
            sbs_load(sbs, role, week, steps, maximum)
        }
        GymMesocycle::Test(test) => test_load(test, consulted, role, inheritance, steps),
    };
    let plan = match plan {
        Ok(plan) => plan,
        Err(reason) => {
            return Derived::underivable(UnderivableSlot {
                slot,
                exercise: exercise.as_str(),
                reason,
            });
        }
    };

    let sets = primary_sets(plan, consulted, role, steps);

    let Ok(sets) = NonEmpty::new(sets) else {
        return Derived::underivable(UnderivableSlot {
            slot,
            exercise: exercise.as_str(),
            reason: UnderivableReason::NoWorkingSet,
        });
    };
    Derived::item(PrescribedItem::Exercise {
        slot,
        exercise: PrescribedExercise::ForReps {
            exercise: *reps_exercise,
            sets,
        },
    })
}

/// A linear programme's rung.
///
/// **The calendar says whether, and the record says which.** A climbing week
/// takes its load from where the progression has got to, not from the week the
/// date falls in — those agree until the first miss and diverge after it.
///
/// A linear calendar emits nothing but climbing weeks since decision 0013, so
/// the test arm below is the type system's edge rather than a state to reach.
fn linear_load(
    linear: &Linear,
    consulted: &Consulted<'_>,
    role: SessionRole,
    week: WeekKind,
    standing: Standing,
    steps: &LoadSteps,
) -> Result<PrimaryLoad, UnderivableReason> {
    let Some(opening) = standing.opening else {
        return Err(UnderivableReason::NoLadder);
    };
    let (Some(progress), Ok(ladder)) = (
        standing.progress,
        linear.ladder(opening, consulted.parameters),
    ) else {
        return Err(UnderivableReason::NoLadder);
    };
    let WeekKind::Climbing(_) = week else {
        return Err(UnderivableReason::NoLadder);
    };
    // **On the intensity alone**, which is the whole of what separates the two
    // top sets: the lighter session's is a share of the heavier session's, and
    // how long either session is does not enter into the load.
    let load = match role.intensity() {
        Relative::Higher => progress.heavy_top_set(ladder, steps),
        Relative::Lower => {
            progress.light_top_set(ladder, steps, consulted.parameters.light_of_heavy)
        }
    };
    load.map_or(Err(UnderivableReason::NoLadder), |load| {
        Ok(PrimaryLoad::TopSet {
            load,
            reps: consulted.programming.top_set_reps.get(role).as_rep_count(),
        })
    })
}

/// A block's week, as its own phase plan states it.
///
/// **Nothing here reads the record.** Every load in a block is a share of the
/// anchor decided by the duration and three literature constants, which is what
/// makes the whole block computable in advance. A miss does not hold it, because
/// there is no ladder position to hold.
/// What an SBS session's primary runs, off the maximum current this week.
///
/// **The maximum moves inside the cycle, so this cannot read the anchor for
/// weeks two onward.** Each repetition-maximum day resets it, and the reset is a
/// function of what was lifted — so the load for week three is not derivable
/// from the authored programme alone. Until the store can answer "what did the
/// last rep-max day produce", this derives from the anchor and is right only in
/// week one.
///
/// That limitation is deliberate and visible rather than papered over: the
/// alternative is to invent a progression the chart does not state.
fn sbs_load(
    _sbs: &Sbs,
    role: SessionRole,
    week: WeekKind,
    steps: &LoadSteps,
    maximum: Option<Kg>,
) -> Result<PrimaryLoad, UnderivableReason> {
    let WeekKind::Climbing(index) = week else {
        return Err(UnderivableReason::NoLadder);
    };
    // The chart's two days in the order it writes them: the lighter session
    // first, the repetition-maximum day second.
    let session = match role.intensity() {
        Relative::Lower => SbsSession::First,
        Relative::Higher => SbsSession::Second,
    };
    let Ok(day) = sbs_day(index.as_u32(), session) else {
        return Err(UnderivableReason::NoLadder);
    };

    // **What the record says the cycle is at.** Resolving it needs the store,
    // which is the caller's business, so an unresolved one is underivable here
    // rather than opened from a guess. There is no authored number to fall back
    // to: a cycle states shares and nothing else.
    let Some(maximum) = maximum else {
        return Err(UnderivableReason::NoOpeningMaximum);
    };

    chart_load(day, maximum, steps)
}

/// One day of the chart, as a load, given the maximum it is a share of.
///
/// **Separate from [`sbs_load`] because two templates read the chart.** A cycle
/// asks for the day its own week and role name, and a provided test week asks
/// for the taper that precedes its attempt; what neither of them decides is
/// what a chart day means once the maximum is known, which is this.
fn chart_load(
    day: SbsDay,
    maximum: Kg,
    steps: &LoadSteps,
) -> Result<PrimaryLoad, UnderivableReason> {
    let increment = steps.step_at(maximum);

    match day {
        SbsDay::Percentage { sets, reps, share } => {
            let Some(load) = working_load(maximum, share, increment) else {
                return Err(UnderivableReason::NoLadder);
            };
            Ok(PrimaryLoad::Across { load, sets, reps })
        }
        SbsDay::RepMax {
            reps,
            back_off_sets,
            back_off_reps,
        } => {
            // What the ramp builds toward: this maximum expressed at the
            // repetition count being attempted, through SBS's own table. The
            // exact inverse of the advance the result will produce.
            let Some(share) = training_max_share(reps) else {
                return Err(UnderivableReason::NoLadder);
            };
            let Some(toward) = working_load(maximum, share, increment) else {
                return Err(UnderivableReason::NoLadder);
            };
            Ok(PrimaryLoad::RepMax {
                toward,
                reps,
                back_off_sets,
                back_off_reps,
            })
        }
        SbsDay::Test { reps } => Ok(PrimaryLoad::Attempt {
            toward: maximum,
            reps,
        }),
    }
}

fn block_load(
    block: &BlockPeriodisation,
    role: SessionRole,
    week: WeekKind,
    opening: Option<Anchor>,
    steps: &LoadSteps,
) -> Result<PrimaryLoad, UnderivableReason> {
    // The calendar reports every week as a climbing one; which of them is a test
    // is the block's business, not the calendar's.
    let WeekKind::Climbing(index) = week else {
        return Err(UnderivableReason::NoLadder);
    };
    let Some(planned) = block.week(index) else {
        return Err(UnderivableReason::NoLadder);
    };
    let Some(anchor) = opening.map(Anchor::load) else {
        return Err(UnderivableReason::NoOpeningMaximum);
    };
    let planned = match planned {
        // **The week the block measures what it is about to plan from.** The ramp
        // builds toward the maximum in force when the block opened, expressed at
        // the repetition count the attempt is performed at — so a triple works
        // up to the 3RM the record implies rather than to a one-rep maximum
        // nobody is attempting. The week is where it finds out whether that was
        // still right.
        BlockWeek::Entry(test) => {
            if role == BLOCK_ENTRY_TEST_ROLE {
                let Some(share) = rep_max(test.reps()) else {
                    return Err(UnderivableReason::NoLadder);
                };
                return Ok(PrimaryLoad::Attempt {
                    toward: steps.quantise_loaded(share.of(anchor)),
                    reps: test.reps(),
                });
            }
            // The other session of that week, at the load the block states for
            // it. Absent means the operator does not run it: there is no honest
            // derivation for a light session of a lift whose maximum this week
            // is about to measure.
            let Some(load) = test.light() else {
                return Err(UnderivableReason::NoEntryTestLightLoad);
            };
            return Ok(PrimaryLoad::TopSet {
                load: steps.quantise_loaded(load),
                reps: test.reps(),
            });
        }
        BlockWeek::Planned(planned) => planned,
    };
    match planned {
        WeekPlan::Working {
            sets, reps, load, ..
        } => {
            let load = steps.quantise_loaded(load.of(anchor));
            if sets.as_u32() == 1 {
                Ok(PrimaryLoad::TopSet { load, reps })
            } else {
                Ok(PrimaryLoad::Across { load, sets, reps })
            }
        }
        WeekPlan::ExitTest { reps, expected } => Ok(PrimaryLoad::Attempt {
            toward: steps.quantise_loaded(expected.of(anchor)),
            reps,
        }),
    }
}

/// A test's week: the attempt on the heavy session, and on the light one either
/// the published week's own other session or the predecessor's.
///
/// **A provided test week is a microcycle of a chart, and the chart states both
/// its days.** *Squat 2x Int* µ4 is a taper and a one-repetition maximum, so the
/// light session is neither underivable nor the predecessor's — it is the taper,
/// a share of the maximum like every other percentage day. What it lacks is the
/// maximum, and the operator settled that on 2026-09-09: *"the light SBS session
/// before a heavy SBS session in a test week should take the heavy's expected
/// target as it's anchor."*
///
/// That target is the number the attempt is an attempt at, so the two sessions
/// of the week are shares of one figure and the week hangs together. Where it
/// was declared it is an assertion — the operator, on the same day: *"I asserted
/// that I think i'm going to hit 95 for 1 in that test"* — and
/// [`TestTarget::Declared`] is the record of that, so nothing here needs to
/// carry a provenance beside it.
///
/// **An unprovided test still runs the predecessor's session.** A week written
/// by nobody has no chart to read a taper off, which is the case
/// [`UnderivableReason::NoPredecessor`] remains for.
fn test_load(
    test: &Test,
    consulted: &Consulted<'_>,
    role: SessionRole,
    inheritance: Inheritance,
    steps: &LoadSteps,
) -> Result<PrimaryLoad, UnderivableReason> {
    if role == Test::ROLE {
        let Some(target) = inheritance.target else {
            return Err(UnderivableReason::NoTarget);
        };
        return Ok(PrimaryLoad::Attempt {
            toward: steps.quantise_loaded(target),
            reps: test.reps(),
        });
    }

    if let Some(day) = provided_other_session(test) {
        // The same refusal the attempt itself makes, and for the same reason:
        // both days of the week are shares of the target, so a week with no
        // target has no light session either.
        let Some(target) = inheritance.target else {
            return Err(UnderivableReason::NoTarget);
        };
        return chart_load(day, target, steps);
    }

    // Not the test, and nothing published to read: the week's other session is
    // the predecessor's, run at the load its progression stands at. A test with
    // nothing before it has no such load, which is why a test that runs a second
    // session needs a predecessor even where its target was declared.
    let Some(load) = inheritance.light else {
        return Err(UnderivableReason::NoPredecessor);
    };
    Ok(PrimaryLoad::TopSet {
        load: steps.quantise_loaded(load),
        reps: consulted.programming.top_set_reps.get(role).as_rep_count(),
    })
}

/// The chart day the other session of a provided test week runs.
///
/// **The first microcycle it took, and a test week takes exactly one.** The
/// selection is `NonEmpty` because a provided *cycle* is four weeks; a test is
/// one, so the first number is the only number.
///
/// [`SbsSession::First`] rather than the role, because which session of the week
/// this is has already been decided: the attempt is
/// [`Test::ROLE`] and this is the other one.
fn provided_other_session(test: &Test) -> Option<SbsDay> {
    let microcycle = test.provided()?.microcycles().next()?;
    sbs_day(microcycle, SbsSession::First).ok()
}

/// Several slots issued as one item, as the template groups them.
///
/// Two slots and then the rest, mirroring `AtLeastTwo`, so "a group has at least
/// two members" is in the signature.
///
/// Any member failing costs the group — issuing part of a supersetted pair
/// would be prescribing something the template does not describe — and every
/// slot in it is then reported, the failure with its own reason and the rest as
/// withheld.
fn group(
    programme: &GymMesocycle,
    consulted: &Consulted<'_>,
    role: SessionRole,
    history: &BTreeMap<RepsExercise, LastPerformance>,
    first: SlotId,
    second: SlotId,
    rest: &[SlotId],
) -> Derived {
    let slots = [first, second].into_iter().chain(rest.iter().copied());
    let derived: Vec<(SlotId, Result<PrescribedExercise, UnderivableSlot>)> = slots
        .map(|slot| {
            (
                slot,
                accessory_exercise(programme, consulted, role, history, slot),
            )
        })
        .collect();

    if derived.iter().any(|(_, member)| member.is_err()) {
        // Every slot that went with it is owed a reason, not just the one that
        // failed: the others are absent from the session too.
        return Derived::Underivable(
            derived
                .into_iter()
                .map(|(slot, member)| match member {
                    Err(reason) => reason,
                    Ok(exercise) => UnderivableSlot {
                        slot,
                        exercise: exercise.exercise_key(),
                        reason: UnderivableReason::GroupWithheld,
                    },
                })
                .collect(),
        );
    }

    let mut members = derived.into_iter().filter_map(|(slot, member)| {
        member
            .ok()
            .map(|exercise| SupersetMember { slot, exercise })
    });
    let (Some(first), Some(second)) = (members.next(), members.next()) else {
        // Unreachable: two slots are named in the signature and neither failed.
        return Derived::Underivable(Vec::new());
    };
    Derived::item(PrescribedItem::Superset(PrescribedSuperset {
        members: AtLeastTwo::of(first, second, members.collect()),
    }))
}

/// Any slot that is not the primary: double progression, a hold, or static.
fn accessory_slot(
    programme: &GymMesocycle,
    consulted: &Consulted<'_>,
    role: SessionRole,
    history: &BTreeMap<RepsExercise, LastPerformance>,
    slot: SlotId,
) -> Derived {
    match accessory_exercise(programme, consulted, role, history, slot) {
        Ok(exercise) => Derived::item(PrescribedItem::Exercise { slot, exercise }),
        Err(reason) => Derived::underivable(reason),
    }
}

/// What a non-primary slot prescribes, before it is placed in an item.
///
/// Separate from [`accessory_slot`] because a supersetted position needs the
/// exercise without the item wrapped around it.
fn accessory_exercise(
    programme: &GymMesocycle,
    consulted: &Consulted<'_>,
    role: SessionRole,
    history: &BTreeMap<RepsExercise, LastPerformance>,
    slot: SlotId,
) -> Result<PrescribedExercise, UnderivableSlot> {
    match programme.fills().content(slot, role) {
        // Authored outright: no history is read and none is needed.
        SlotContent::Static(fill) => static_exercise(fill).map_err(|reason| UnderivableSlot {
            slot,
            exercise: fill.exercise.as_str(),
            reason,
        }),
        SlotContent::Single(exercise) => one_exercise(consulted, history, *exercise, slot),
    }
}

/// The scheme a block's non-primary slots run.
const fn scheme_for<'a>(
    consulted: &Consulted<'a>,
    block: Block,
) -> &'a domain::prescription::AccessoryScheme {
    match block {
        Block::Hypertrophy => &consulted.programming.hypertrophy,
        _ => &consulted.programming.strength,
    }
}

/// How a slot's numbers are arrived at.
///
/// A total function of `(slot, primacy)`, which is what the model of record says:
/// the primary gets a top set and back-offs, every other strength and hypertrophy
/// slot gets double progression, and the plyometric, power and mobility blocks are
/// static. A slot therefore collapses to just an exercise, and a primary-style
/// scheme on a non-primary slot is unwritable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheme {
    /// Prescribed outright by the programme. No progression and no history: a
    /// static slot is set at the start of the block, so reading the last
    /// performance would let a bad session re-issue itself.
    Static,
    /// A hold, for the authored length, every time.
    Hold,
    DoubleProgression,
}

const fn scheme_of(slot: SlotId) -> Scheme {
    match slot.block() {
        Block::Plyometric | Block::Power => Scheme::Static,
        Block::Mobility => Scheme::Hold,
        Block::Strength | Block::Hypertrophy => Scheme::DoubleProgression,
    }
}

/// A static hold: the authored duration, once per side.
///
/// Once for a position held on both sides at once, twice for one worked a side
/// at a time — a couch stretch is sixty seconds *per leg*, and issuing it as a
/// single set prescribes half the work. The duration is the authored one either
/// way; what the exercise decides is how many times it is held.
fn hold(consulted: &Consulted<'_>, exercise: DurationExercise) -> PrescribedExercise {
    let set = || {
        PrescribedSet::fixed(
            // Unloaded, and the pinned axis is volume rather than intensity —
            // which is how a slot with no load still prescribes something.
            Load::UNLOADED,
            Target::Exactly(consulted.parameters.static_hold),
        )
    };
    let rest = (1..exercise.sides().holds()).map(|_| set()).collect();
    PrescribedExercise::ForDuration {
        exercise,
        sets: NonEmpty::of(set(), rest),
    }
}

/// A static slot, exactly as the programme prescribes it.
fn static_exercise(
    fill: &domain::prescription::StaticFill,
) -> Result<PrescribedExercise, UnderivableReason> {
    let Exercise::Reps(exercise) = fill.exercise else {
        return Err(UnderivableReason::NotCountedInReps);
    };
    let sets = (0..fill.sets.as_u32())
        .map(|_| PrescribedSet::fixed(Load::UNLOADED, Target::Exactly(fill.reps)))
        .collect();
    Ok(PrescribedExercise::ForReps {
        exercise,
        sets: NonEmpty::new(sets).map_err(|_| UnderivableReason::NoWorkingSet)?,
    })
}

/// One exercise's sets, by double progression against its own last performance.
fn one_exercise(
    consulted: &Consulted<'_>,
    history: &BTreeMap<RepsExercise, LastPerformance>,
    exercise: Exercise,
    slot: SlotId,
) -> Result<PrescribedExercise, UnderivableSlot> {
    let underivable = |reason| UnderivableSlot {
        slot,
        exercise: exercise.as_str(),
        reason,
    };

    // A hold needs no history at all: it is the authored duration, every time.
    // A static slot never reaches here — `accessory_slot` prescribes it outright.
    if scheme_of(slot) == Scheme::Hold {
        let Exercise::Duration(duration_exercise) = exercise else {
            return Err(underivable(UnderivableReason::NotAHold));
        };
        return Ok(hold(consulted, duration_exercise));
    }

    let Exercise::Reps(reps_exercise) = exercise else {
        return Err(underivable(UnderivableReason::NotCountedInReps));
    };
    let Some(LastPerformance::Performed(last)) = history.get(&reps_exercise) else {
        return Err(underivable(UnderivableReason::NeverPerformed));
    };
    let scheme = scheme_for(consulted, slot.block());
    let load = progressed_load(consulted, exercise, scheme, last).map_err(underivable)?;
    let sets: Vec<_> = (0..scheme.sets.as_u32())
        .map(|_| PrescribedSet::fixed(load, scheme.reps))
        .collect();

    let sets = NonEmpty::new(sets).map_err(|_| underivable(UnderivableReason::NoWorkingSet))?;
    Ok(PrescribedExercise::ForReps {
        exercise: reps_exercise,
        sets,
    })
}

/// The heaviest of a performed exercise's sets.
///
/// **The load a progression is against is the heaviest one, not the last one**
/// (issue #129). Two shapes make those differ, and both are ordinary: a top set
/// followed by back-offs, which is what the primary template issues and what an
/// accessory takes whenever the operator works down; and ascending assistance,
/// which is how the dips and pull-ups in the record are logged — `-7`, `-14`,
/// `-21` across three sets as fatigue accumulates. In each the last set is the
/// easiest, so progressing from it re-issues or steps up from the wrong number.
///
/// **A pair on two axes keeps the incumbent.** [`Load`] is only partially
/// ordered, because an absolute load and a relative one are not comparable — and
/// an exercise's implement decides its axis, so no performed exercise in the
/// record has ever carried both. Keeping the first axis seen is a decision about
/// a state the record does not reach, made explicitly rather than by whichever
/// way a comparison happened to fall.
fn heaviest_of<'a>(sets: &[&'a PerformedSetSummary]) -> Option<&'a PerformedSetSummary> {
    sets.iter()
        .copied()
        .fold(None, |heaviest, set| match heaviest {
            Some(held) if held.load.partial_cmp(&set.load) != Some(Ordering::Less) => Some(held),
            Some(_) | None => Some(set),
        })
}

/// Double progression: work the range, and add an increment once the top of it
/// was reached at every working set.
///
/// A failed attempt is not the top of the range, so a session that failed
/// re-issues rather than advancing — which is the same rule the primary's gate
/// runs, arrived at from the other direction.
///
/// **The scale is read only when the load actually moves.** A slot working its
/// way up a range re-issues what it last did, and re-issuing a weight that was
/// on the bar last week needs no opinion about what else the equipment can
/// hold. So an implement with no authored scale costs a slot nothing until the
/// week it would have stepped up — which is the week somebody has to state the
/// scale anyway.
fn progressed_load(
    consulted: &Consulted<'_>,
    exercise: Exercise,
    scheme: &domain::prescription::AccessoryScheme,
    last: &Performance,
) -> Result<Load, UnderivableReason> {
    // **The one place a set's kind is read** (issue #127). Everything else about
    // the record answers "what was lifted", which does not depend on what the
    // source called a set; this asks whether every set the range was prescribed
    // for reached the top of it, and a warm-up is not one of those. Left in, a
    // ramp's three repetitions answer "no" every week and hold the load where it
    // is for good — a freeze rather than a week's delay, because next week's
    // ramp answers "no" again.
    let working: Vec<_> = last
        .sets
        .iter()
        .filter(|set| set.kind == SetKind::Working)
        .collect();
    let heaviest = heaviest_of(&working).ok_or(UnderivableReason::NoWorkingSet)?;
    let reached_top = working.iter().all(|set| {
        set.outcome
            .completed()
            .is_some_and(|reps| *reps >= scheme.reps.maximum())
    });

    if !reached_top {
        return Ok(heaviest.load);
    }

    let steps = consulted
        .parameters
        .scales
        .for_exercise(exercise)
        .ok_or(UnderivableReason::NoLoadScale)?;
    match heaviest.load {
        // The step is read at the load being left, so a dumbbell leaving 10kg
        // adds the 2kg that applies from 10kg rather than the 1kg that got it
        // there.
        Load::Absolute(mass) => Ok(Load::Absolute(steps.next_above(mass))),
        // A relative load progresses the same way, on the axis it runs on. The
        // step is read at the magnitude, because an implement's scale is about
        // what it can hold and not which direction the load points.
        Load::Relative(delta) => {
            let magnitude = Kg::from_grams(delta.as_grams().unsigned_abs());
            let step = steps.step_at(magnitude).as_grams();
            Ok(Load::Relative(domain::gym::SignedKg::from_grams(
                delta
                    .as_grams()
                    .saturating_add(i64::try_from(step).unwrap_or(i64::MAX)),
            )))
        }
    }
}

/// Storing an authored plan and the consulted it was generated against.
pub struct Authoring<P, G> {
    plans: P,
    parameters: G,
}

impl<P, G> Authoring<P, G> {
    pub const fn new(plans: P, parameters: G) -> Self {
        Self { plans, parameters }
    }
}

impl<P, G> PlanAuthor for Authoring<P, G>
where
    P: PlanStore + Sync,
    G: GenerationParameterStore + Sync,
{
    async fn author(
        &self,
        plan: &Plan,
        parameters: &GenerationParameters,
    ) -> Result<(PlanId, Authored), PrescriptionError> {
        // Refused before anything is written. Two plans covering one day would
        // make which of them answers depend on the order rows came back in,
        // which is the silent ambiguity § 12's discipline exists to stop.
        // Versions of one plan never conflict — `overlaps` knows that a shared
        // name means a re-authoring rather than a rival.
        let proposed = plan.window();
        let mut authored = Authored::Created;
        for existing in self.plans.windows().await? {
            if existing.name() == proposed.name() {
                authored = Authored::Modified;
                continue;
            }
            if proposed.overlaps(&existing) {
                return Err(PrescriptionError::OverlappingPlan { proposed, existing });
            }
        }

        // **Nothing here reads another mesocycle any more.** A block used to be
        // able to claim it opened from a test that had already happened, and
        // that claim had to be checked against the store; no programme states a
        // maximum now, so there is no claim left to be wrong.
        // Parameters first: a plan names the version it was authored against,
        // and one stored without them would reference nothing.
        self.parameters
            .author(plan.authored_at(), parameters)
            .await?;
        Ok((self.plans.author(plan).await?, authored))
    }
}

/// The primary slot's sets: the warm-up ramp, then whatever the plan asks for.
///
/// Lifted out of [`primary_slot_item`] when SBS's repetition-maximum day made
/// that function too long to read in one go. It is the half that turns a
/// [`PrimaryLoad`] into sets, and it needs nothing about the programme.
fn primary_sets(
    plan: PrimaryLoad,
    consulted: &Consulted<'_>,
    role: SessionRole,
    steps: &LoadSteps,
) -> Vec<PrescribedSet<RepCount>> {
    let mut sets: Vec<PrescribedSet<RepCount>> = Vec::new();
    // The ramp is a share of what the session is working toward, whatever that
    // is — never of the anchor. Ramping off the anchor had the operator warming
    // up toward a number they had passed three weeks earlier (decision 0011).
    let toward = match plan {
        PrimaryLoad::TopSet { load, .. } | PrimaryLoad::Across { load, .. } => load,
        PrimaryLoad::RepMax { toward, .. } => toward,
        // A test's ramp leads into its first attempt, which is the next thing
        // loaded — not the target two steps above it (#136).
        PrimaryLoad::Attempt { toward, .. } => Attempts::toward(toward, steps).first(),
    };
    // **The ramp's repetition counts follow the top set's** (decision 0030), but
    // only where the top set is maximal: a percentage day states a submaximal
    // load, so its ramp has nothing to rehearse. The stored ramp is the floor in
    // both cases, so a low count leaves it exactly as authored.
    let ramp = match plan {
        PrimaryLoad::RepMax { reps, .. } | PrimaryLoad::Attempt { reps, .. } => {
            Cow::Owned(warmup_ramp(&consulted.programming.warmup, reps))
        }
        PrimaryLoad::TopSet { .. } | PrimaryLoad::Across { .. } => {
            Cow::Borrowed(&consulted.programming.warmup)
        }
    };
    for step in ramp.iter() {
        sets.push(PrescribedSet::warmup(
            Load::Absolute(steps.quantise_loaded(step.of_top_set.of(toward))),
            Target::Exactly(step.reps),
        ));
    }

    match plan {
        PrimaryLoad::TopSet { load, reps } => {
            sets.push(PrescribedSet::fixed(
                Load::Absolute(load),
                Target::Exactly(reps),
            ));
            // The back-offs are the role's own pattern — heavy `2 × 4`, light
            // `3 × 6` — and not the strength block's accessory scheme.
            let pattern = consulted.programming.back_off.get(role);
            let back_off = steps.quantise_loaded(pattern.of_top_set.of(load));
            for _ in 0..pattern.sets.as_u32() {
                sets.push(PrescribedSet::fixed(
                    Load::Absolute(back_off),
                    Target::Exactly(pattern.reps),
                ));
            }
        }
        PrimaryLoad::Across {
            load,
            sets: across,
            reps,
        } => {
            for _ in 0..across.as_u32() {
                sets.push(PrescribedSet::fixed(
                    Load::Absolute(load),
                    Target::Exactly(reps),
                ));
            }
        }
        PrimaryLoad::Attempt { toward, reps } => {
            // **The target is in the plan, so it is printed.** Decision 0011
            // keeps it out of `programme show`, and that still holds: there it
            // is a *projection* for a week that has not happened, and every
            // session between now and then can move it. A prescription is issued
            // for one date against the record as it stands, which is the moment
            // the number is knowable.
            //
            // **Three attempts, and nothing past the last** (#136). The target
            // is a guess, so the two below it are what a miss leaves behind, and
            // zero in reserve is asked of the target alone. A lift past it is
            // the record's to take, not the plan's to ask for.
            let [first, second, target] = Attempts::toward(toward, steps).loads();
            for load in [first, second] {
                sets.push(PrescribedSet::fixed(
                    Load::Absolute(load),
                    Target::Exactly(reps),
                ));
            }
            sets.push(
                PrescribedSet::fixed(Load::Absolute(target), Target::Exactly(reps))
                    .with_effort(domain::gym::Rir::Zero),
            );
        }
        PrimaryLoad::RepMax {
            toward,
            reps,
            back_off_sets,
            back_off_reps,
        } => {
            // **A set, not a work-up.** The working up is done by the time the
            // bar is loaded: decision 0030's ramp reaches `n − 3` repetitions at
            // 90% of this load, so what remains is one set at a stated weight
            // taken to nothing in reserve. Prescribing it as autoregulated —
            // load open, work up to it — described a session that no longer
            // happens, and hid the number the chart had already derived.
            //
            // Working up re-enters only if this set turns out *not* to be at
            // zero in reserve, and that is a decision in the room. The record
            // carries what was actually lifted and `maximum_after` reads it, so
            // nothing is lost by the plan declining to predict it.
            sets.push(
                PrescribedSet::fixed(Load::Absolute(toward), Target::Exactly(reps))
                    .with_effort(domain::gym::Rir::Zero),
            );
            // The chart's `3 × 5–6 @ 8RM`, at the same load. Autoregulating
            // these said they were taken to failure, which the chart does not
            // ask and the operator does not do.
            for _ in 0..back_off_sets.as_u32() {
                sets.push(PrescribedSet::fixed(Load::Absolute(toward), back_off_reps));
            }
        }
    }
    sets
}

#[cfg(test)]
mod progression_tests {
    //! Double progression progresses from the heaviest working set (issue #129).
    //!
    //! Here rather than in `infrastructure/tests` because [`progressed_load`] is
    //! private and needs no port: it is a rule about one performance.

    use domain::{
        gym::{Load, Performed, SetKind, SignedKg, exercise::Exercise},
        landing::LandingRecordId,
        measure::{Kg, RepCount},
        prescription::seed::seed,
    };
    use jiff::civil::Date;

    use super::{Consulted, programming};
    use super::{Performance, PerformedSetSummary, progressed_load};

    /// One completed set. Returns `Result`: the test exemptions do not reach a
    /// helper defined beside a `#[test]`.
    fn set(load: Load, reps: u32) -> Result<PerformedSetSummary, Box<dyn std::error::Error>> {
        Ok(PerformedSetSummary {
            load,
            outcome: Performed::Completed(RepCount::new(reps)?),
            kind: SetKind::Working,
        })
    }

    fn performance(
        sets: Vec<PerformedSetSummary>,
    ) -> Result<Performance, Box<dyn std::error::Error>> {
        Ok(Performance {
            on: Date::constant(2026, 8, 3),
            landed_as: LandingRecordId::try_from(1)?,
            fulfilled: None,
            sets,
        })
    }

    /// The shape the primary template issues: a top set, then lighter back-offs.
    ///
    /// Every set reached the top of the 4-6 range, so the load steps up — from
    /// the 40kg top set, not from the 30kg back-off it ended on.
    #[test]
    fn a_session_progresses_from_its_top_set_not_its_back_offs() {
        let parameters = seed().expect("the seed builds");
        let shape = programming().expect("the shipped programming builds");
        let consulted = Consulted {
            parameters: &parameters,
            programming: &shape,
        };
        let last = performance(vec![
            set(Load::absolute(Kg::from_grams(40_000)), 6).expect("a set"),
            set(Load::absolute(Kg::from_grams(30_000)), 6).expect("a set"),
            set(Load::absolute(Kg::from_grams(30_000)), 6).expect("a set"),
        ])
        .expect("a performance");

        let progressed = progressed_load(
            &consulted,
            Exercise::Reps(domain::gym::exercise::RepsExercise::PreacherCurlBarbell),
            &consulted.programming.strength,
            &last,
        )
        .expect("the slot derives");

        assert_eq!(
            progressed,
            Load::absolute(Kg::from_grams(42_500)),
            "the 40kg top set steps to 42.5, and the back-offs are not the load"
        );
    }

    /// How the dips and pull-ups in the record are logged: assistance increases
    /// across the session as fatigue accumulates, so the last set is the easiest.
    ///
    /// **The real shape, taken from 2026-06-15**: `-7 x 4`, `-14 x 5`, `-21 x 6`.
    /// It fell short of the top of the range, so the load re-issues — and what
    /// re-issues is the `-7` it started at, not the `-21` it ended on. Stepping
    /// up is not reachable here: the seed authors no scale for a bodyweight
    /// implement, so the week this slot would step is the week somebody has to
    /// state one.
    #[test]
    fn an_assisted_session_re_issues_its_least_assisted_set() {
        let parameters = seed().expect("the seed builds");
        let shape = programming().expect("the shipped programming builds");
        let consulted = Consulted {
            parameters: &parameters,
            programming: &shape,
        };
        let last = performance(vec![
            set(Load::relative(SignedKg::from_grams(-7_000)), 4).expect("a set"),
            set(Load::relative(SignedKg::from_grams(-14_000)), 5).expect("a set"),
            set(Load::relative(SignedKg::from_grams(-21_000)), 6).expect("a set"),
        ])
        .expect("a performance");

        let progressed = progressed_load(
            &consulted,
            Exercise::Reps(domain::gym::exercise::RepsExercise::ChestDip),
            &consulted.programming.strength,
            &last,
        )
        .expect("the slot derives");

        assert_eq!(
            progressed,
            Load::relative(SignedKg::from_grams(-7_000)),
            "less assistance is heavier, so -7 is the set the session is measured at"
        );
    }

    /// A session that fell short re-issues the heaviest set, not the last one.
    #[test]
    fn a_session_that_fell_short_re_issues_its_top_set() {
        let parameters = seed().expect("the seed builds");
        let shape = programming().expect("the shipped programming builds");
        let consulted = Consulted {
            parameters: &parameters,
            programming: &shape,
        };
        let last = performance(vec![
            set(Load::absolute(Kg::from_grams(40_000)), 6).expect("a set"),
            set(Load::absolute(Kg::from_grams(30_000)), 4).expect("a set"),
        ])
        .expect("a performance");

        let progressed = progressed_load(
            &consulted,
            Exercise::Reps(domain::gym::exercise::RepsExercise::PreacherCurlBarbell),
            &consulted.programming.strength,
            &last,
        )
        .expect("the slot derives");

        assert_eq!(progressed, Load::absolute(Kg::from_grams(40_000)));
    }
}

#[cfg(test)]
mod measurement_tests {
    //! What a test week leaves behind, read off a session that missed (issue
    //! #127).
    //!
    //! Here rather than in `infrastructure/tests` because [`heaviest_completed`]
    //! is private and needs no port: it is a rule about a slice of performances.
    //! The three sessions below are the shapes the real record holds — the front
    //! squat test of 2026-07-03, whose ramp is tagged working; a back squat day
    //! of June 2025, whose heaviest completed set is a bridging single tagged
    //! warm-up; and the light session that used to answer for both.

    use domain::{
        gym::{Load, Performed, SetKind},
        landing::LandingRecordId,
        measure::{Kg, RepCount},
        plan::Span,
    };
    use jiff::civil::Date;

    use super::{Performance, PerformedSetSummary, heaviest_completed};

    /// One set, as the record holds it. Returns `Result`: the test exemptions do
    /// not reach a helper defined beside a `#[test]`.
    fn set(
        kg: u64,
        reps: Option<u32>,
        kind: SetKind,
    ) -> Result<PerformedSetSummary, Box<dyn std::error::Error>> {
        Ok(PerformedSetSummary {
            load: Load::Absolute(Kg::from_grams(kg)),
            outcome: match reps {
                Some(count) => Performed::Completed(RepCount::new(count)?),
                None => Performed::Failed,
            },
            kind,
        })
    }

    fn performance(
        on: Date,
        id: i64,
        sets: Vec<PerformedSetSummary>,
    ) -> Result<Performance, Box<dyn std::error::Error>> {
        Ok(Performance {
            on,
            landed_as: LandingRecordId::try_from(id)?,
            fulfilled: None,
            sets,
        })
    }

    /// The week the test is in: Monday 29 June 2026 to Sunday 5 July.
    fn week() -> Span {
        Span::new(Date::constant(2026, 6, 29), 1)
    }

    #[test]
    fn a_failed_attempt_is_not_what_was_lifted() {
        let day = performance(
            Date::constant(2026, 7, 3),
            1,
            vec![
                set(72_500, Some(1), SetKind::Warmup).expect("a ramp step"),
                set(80_000, Some(1), SetKind::Working).expect("a single"),
                set(90_000, Some(1), SetKind::Working).expect("a single"),
                set(95_000, None, SetKind::Working).expect("a failed attempt"),
            ],
        )
        .expect("a performance");

        assert_eq!(
            heaviest_completed(&[day], week()),
            Some(Kg::from_grams(90_000)),
            "the completed single below the failure is what was lifted, and the \
             session's heaviest set is the failure"
        );
    }

    #[test]
    fn the_lighter_session_does_not_answer_for_the_test() {
        let taper = performance(
            Date::constant(2026, 6, 30),
            1,
            vec![
                set(70_000, Some(3), SetKind::Working).expect("a triple"),
                set(70_000, Some(3), SetKind::Working).expect("a triple"),
            ],
        )
        .expect("a performance");
        let test = performance(
            Date::constant(2026, 7, 3),
            2,
            vec![
                set(90_000, Some(1), SetKind::Working).expect("a single"),
                set(95_000, None, SetKind::Working).expect("a failed attempt"),
            ],
        )
        .expect("a performance");

        assert_eq!(
            heaviest_completed(&[taper, test], week()),
            Some(Kg::from_grams(90_000)),
            "the taper is 70kg and the test reached 90"
        );
    }

    #[test]
    fn a_ramp_tagged_warm_up_is_still_what_was_lifted() {
        // The June 2025 back squat shape: the ramp is the heavy work and the
        // working sets below it are volume.
        let day = performance(
            Date::constant(2026, 7, 3),
            1,
            vec![
                set(82_500, Some(1), SetKind::Warmup).expect("a bridging single"),
                set(92_500, Some(1), SetKind::Warmup).expect("a bridging single"),
                set(70_000, Some(10), SetKind::Working).expect("a set of ten"),
            ],
        )
        .expect("a performance");

        assert_eq!(
            heaviest_completed(&[day], week()),
            Some(Kg::from_grams(92_500)),
            "92.5kg went up, whatever the source called the set"
        );
    }

    #[test]
    fn a_session_outside_the_span_is_not_evidence_about_it() {
        let after = performance(
            Date::constant(2026, 7, 6),
            1,
            vec![set(100_000, Some(1), SetKind::Working).expect("a single")],
        )
        .expect("a performance");

        assert_eq!(heaviest_completed(&[after], week()), None);
    }
}

#[cfg(test)]
mod ramp_tests {
    //! The ramp before a maximal top set follows its repetition count
    //! (decision 0030), and the ramp before a submaximal one does not.
    //!
    //! Here rather than in `infrastructure/tests` because [`primary_sets`] is
    //! private and needs no port: it turns a [`PrimaryLoad`] into sets and
    //! nothing else. What an SBS *cycle* prescribes end to end cannot be tested
    //! at all yet — the store's `template` CHECK does not admit `sbs`.

    use super::{Consulted, programming};
    use super::{PrimaryLoad, primary_sets};
    use domain::{
        gym::{Load, Rir},
        measure::{Kg, RepCount},
        prescription::{LoadSteps, seed::seed},
        schedule::{Relative, SessionRole},
    };

    fn reps_of(plan: PrimaryLoad) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
        let parameters = seed()?;
        let shape = programming()?;
        let consulted = Consulted {
            parameters: &parameters,
            programming: &shape,
        };
        let steps = LoadSteps::uniform(Kg::from_grams(2_500))?;
        let sets = primary_sets(
            plan,
            &consulted,
            SessionRole::new(Relative::Higher, Relative::Lower),
            &steps,
        );
        Ok(sets
            .iter()
            .filter(|set| set.warmup)
            .filter_map(|set| match set.prescription {
                domain::prescription::Prescribed::Fixed { measure, .. } => {
                    Some(measure.minimum().as_u32())
                }
                _ => None,
            })
            .collect())
    }

    #[test]
    fn the_eight_rep_maximum_ramps_eight_eight_six_five() {
        let plan = PrimaryLoad::RepMax {
            toward: Kg::from_grams(75_000),
            reps: RepCount::new(8).expect("eight is a repetition count"),
            back_off_sets: RepCount::new(3).expect("three is a set count"),
            back_off_reps: domain::prescription::Target::between(
                RepCount::new(5).expect("five is a repetition count"),
                RepCount::new(6).expect("six is a repetition count"),
            )
            .expect("five to six is a range"),
        };
        assert_eq!(reps_of(plan).expect("the seed builds"), vec![8, 8, 6, 5]);
    }

    #[test]
    fn the_one_rep_test_keeps_the_stored_ramp() {
        let plan = PrimaryLoad::Attempt {
            toward: Kg::from_grams(92_500),
            reps: RepCount::new(1).expect("one is a repetition count"),
        };
        assert_eq!(reps_of(plan).expect("the seed builds"), vec![4, 3, 2, 1]);
    }

    /// The autumn entry test (#136), on the operator's numbers: the ramp is
    /// taken off 90, then 90, 92.5 and 95, and only the last asks for
    /// everything.
    #[test]
    fn a_test_ramps_into_its_first_attempt_and_stops_at_the_target() {
        let plan = PrimaryLoad::Attempt {
            toward: Kg::from_grams(95_000),
            reps: RepCount::new(1).expect("one is a repetition count"),
        };
        let parameters = seed().expect("the seed builds");
        let shape = programming().expect("the shipped programming builds");
        let consulted = Consulted {
            parameters: &parameters,
            programming: &shape,
        };
        let steps = LoadSteps::uniform(Kg::from_grams(2_500)).expect("a barbell is one band");
        let shape: Vec<(bool, Option<u64>, Option<Rir>)> = primary_sets(
            plan,
            &consulted,
            SessionRole::new(Relative::Higher, Relative::Lower),
            &steps,
        )
        .iter()
        .map(|set| {
            let grams = match set.prescription.load() {
                Some(Load::Absolute(kg)) => Some(kg.as_grams()),
                _ => None,
            };
            (set.warmup, grams, set.prescription.effort())
        })
        .collect();
        assert_eq!(
            shape,
            vec![
                (true, Some(35_000), None),
                (true, Some(55_000), None),
                (true, Some(72_500), None),
                (true, Some(80_000), None),
                (false, Some(90_000), None),
                (false, Some(92_500), None),
                (false, Some(95_000), Some(Rir::Zero)),
            ]
        );
    }

    #[test]
    fn a_percentage_day_keeps_the_stored_ramp_however_many_reps_it_runs() {
        // 5 × 5 @ 80% is not a maximum, so its ramp has nothing to rehearse.
        let plan = PrimaryLoad::Across {
            load: Kg::from_grams(74_000),
            sets: RepCount::new(5).expect("five is a set count"),
            reps: RepCount::new(5).expect("five is a repetition count"),
        };
        assert_eq!(reps_of(plan).expect("the seed builds"), vec![4, 3, 2, 1]);
    }
}
