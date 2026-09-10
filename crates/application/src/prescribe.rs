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
        Kg, Load, SetKind,
        exercise::{DurationExercise, Exercise, RepsExercise},
    },
    measure::RepCount,
    normalised::OperatorZone,
    plan::{Occupies, Plan, PlanId, PlanName, Span},
    prescription::{
        Anchor, AnchorProvenance, Anchoring, Block, BlockPeriodisation, BlockWeek, DerivedFrom,
        GatingTopSet, GenerationParameters, Linear, LoadSteps, Mesocycle, Position,
        PrescribedExercise, PrescribedItem, PrescribedSet, PrescribedSuperset, PrescribedWorkout,
        PrescriptionState, Progress, Progression, RECENT_WEEKS, Sbs, SbsDay, SbsSession,
        SessionRole, SlotId, SupersetMember, Target, Test, TestTarget, WeekKind, WeekPlan,
        WorkoutShape, is_recent_enough,
        linear::SlotContent,
        progress_after, rep_max, rested,
        sbs::chart::{
            day as sbs_day, maximum_after as sbs_maximum_after, training_max_share, working_load,
        },
        warmup_ramp,
    },
    sequence::{AtLeastTwo, NonEmpty},
};
use jiff::{Timestamp, civil::Date};

use crate::{
    error::PrescriptionError,
    ports::{
        Authored, ExerciseHistory, GenerationParameterStore, Issuance, LadderStanding,
        LastPerformance, MesocycleStore, Performance, PerformedSetSummary, PlanAuthor, PlanStore,
        PrescribedWorkoutStore, Prescription, PrescriptionLifecycle, UnderivableReason,
        UnderivableSlot, WorkoutPrescriber,
    },
};

/// The date an invocation means when it names none.
///
/// **Capability, and it lives here because both driving adapters need the same
/// answer.** "The next programmed day at or after today" is a statement about
/// the operator's block: it reads the programme in force and asks its calendar.
/// It sat in `cli` until 2026-08-30, which made a decision about training into
/// something built into a transport — a terminal and a browser could have
/// disagreed about which session was next, and nothing would have caught it.
///
/// **A function rather than a method on [`WorkoutPrescriber`]**, because the
/// only port it needs is the programme store. `deliver` and `compare` ask this
/// question too, and neither should have to construct a prescriber — a whole
/// generation apparatus, four ports deep — to find out what day it is asking
/// about.
///
/// The zone is passed rather than read from the programme: finding the
/// programme needs a date, and the calendar that carries a zone is inside the
/// programme. That circularity is why this takes an instant and a zone.
///
/// # Errors
///
/// [`PrescriptionError::NoPlan`] where nothing covers today, and
/// [`PrescriptionError::NoSessionScheduled`] where the block in force has
/// finished. They read differently to an operator and are not merged.
pub async fn next_session(
    programmes: &(impl MesocycleStore + Sync),
    now: Timestamp,
    zone: &OperatorZone,
) -> Result<Date, PrescriptionError> {
    // Today in the operator's zone, because *which programme is in force* is a
    // question about their day. The calendar answers the rest in its own zone,
    // which is the same zone by construction and is its business either way.
    let today = now.to_zoned(zone.as_time_zone()).date();

    let Some((_, _, programme)) = programmes.on(today).await? else {
        return Err(PrescriptionError::NoPlan { date: today });
    };

    // **The calendar answers, not this.** Which days a block runs, which weeks
    // it skips and where it ends are all its own; anything more here would be a
    // second opinion about a schedule that already has one.
    programme
        .calendar()
        .next_session(now)
        .ok_or(PrescriptionError::NoSessionScheduled { from: today })
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
        let progress = self.progress_of(&plan, &programme, &parameters, on).await?;
        let target = self.inheritance(&programme, &parameters, on).await?.target;
        Ok(LadderStanding {
            target,
            plan,
            programme_id,
            programme,
            parameters,
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
        // programme version derived it and under which parameters — every one of
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
    /// Derive the session the programme, the parameters and the record produce
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
            .progress_of(&plan, &programme, &parameters, date)
            .await?;

        // What a test week takes from the programme before it: the target it is
        // an attempt at, and the load its other session runs at. Both are empty
        // for a programme that climbs, which has neither question to ask.
        let inheritance = self.inheritance(&programme, &parameters, date).await?;

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
        let opening = self.opening_anchor(&programme, &parameters).await?;

        let mut items = Vec::new();
        let mut underivable = Vec::new();
        let standing = Standing {
            progress,
            inheritance,
            maximum: self
                .maximum_of(&plan, &programme, &parameters, date)
                .await?,
        };
        for derived in issue_slots(&programme, &parameters, role, week, standing, &history) {
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
        let shape = rested(&WorkoutShape::new(items), &parameters.rest);

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
        programme: &Mesocycle,
        parameters: &GenerationParameters,
        before: Date,
    ) -> Result<Option<Progress>, PrescriptionError> {
        match programme {
            Mesocycle::Progression(Progression::Linear(linear)) => {
                Ok(Some(self.progress(plan, linear, parameters, before).await?))
            }
            // **Neither has a rung.** A block's loads are shares of a fixed
            // anchor; an SBS cycle's are shares of a maximum that moves, but it
            // moves off measured results rather than off a ladder position, so
            // there is still nothing here for a miss to hold.
            Mesocycle::Progression(
                Progression::BlockPeriodisation(_) | Progression::Provided { .. },
            )
            | Mesocycle::Test(_) => Ok(None),
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
        programme: &Mesocycle,
        parameters: &GenerationParameters,
        before: Date,
    ) -> Result<Option<Kg>, PrescriptionError> {
        match programme {
            Mesocycle::Progression(Progression::Provided { cycle: sbs, .. }) => {
                Ok(Some(self.sbs_maximum(plan, sbs, parameters, before).await?))
            }
            Mesocycle::Progression(Progression::Linear(_) | Progression::BlockPeriodisation(_))
            | Mesocycle::Test(_) => Ok(None),
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
    /// What a mesocycle's loads are shares of, resolved.
    ///
    /// `None` for a test, which has a target rather than an anchor, and for a
    /// provided cycle inheriting from a predecessor that measured nothing.
    async fn opening_anchor(
        &self,
        programme: &Mesocycle,
        parameters: &GenerationParameters,
    ) -> Result<Option<Anchor>, PrescriptionError> {
        match programme {
            Mesocycle::Progression(Progression::Provided { cycle: sbs, .. }) => {
                self.opening_of(sbs, parameters).await
            }
            Mesocycle::Progression(periodisation) => Ok(periodisation.anchor()),
            Mesocycle::Test(_) => Ok(None),
        }
    }

    /// What a cycle opens from: the number it states, or the one before it.
    ///
    /// **An inherited opening is resolved against the record, never the plan.**
    /// A cycle plans to leave a maximum behind — week 4 day 2 is a one-repetition
    /// maximum and is not optional — but planning to measure is not measuring. A
    /// predecessor nobody trained leaves nothing, and this answers `None` rather
    /// than reaching for the number that cycle was authored with.
    ///
    /// **What comes back is a `Tested` anchor dated to the predecessor**, because
    /// that is what it is: a measurement, taken on a day the record names. It is
    /// recorded with the prescription like any other, so a session issued in
    /// November says which test it descended from.
    async fn opening_of(
        &self,
        sbs: &Sbs,
        parameters: &GenerationParameters,
    ) -> Result<Option<Anchor>, PrescriptionError> {
        match sbs.entry() {
            Anchoring::Stated(entry) => Ok(Some(entry.anchor())),
            Anchoring::Inherited => {
                let Some((_, before_plan, before)) = self
                    .ports
                    .programmes
                    .preceding(sbs.calendar().start())
                    .await?
                else {
                    return Ok(None);
                };
                self.left_behind(&before_plan, &before, parameters).await
            }
        }
    }

    /// The maximum a mesocycle actually measured, as the record has it.
    ///
    /// **Two things measure one** ([`Mesocycle::produces_maximum`]): a test week,
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
        mesocycle: &'a Mesocycle,
        parameters: &'a GenerationParameters,
    ) -> std::pin::Pin<
        Box<dyn Future<Output = Result<Option<Anchor>, PrescriptionError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let span = mesocycle.span();
            let measured = match mesocycle {
                Mesocycle::Progression(Progression::Provided { cycle: before, .. }) => {
                    // Asked the day after it ends, so its own last session — the
                    // one-repetition maximum — counts toward what it leaves.
                    let after = span.end().tomorrow().unwrap_or_else(|_| span.end());
                    Some(self.sbs_maximum(plan, before, parameters, after).await?)
                }
                Mesocycle::Test(test) => match test.primary_exercise() {
                    Exercise::Reps(primary) => self.measured_in(primary, span).await?,
                    Exercise::Duration(_) | Exercise::Distance(_) => None,
                },
                Mesocycle::Progression(
                    Progression::Linear(_) | Progression::BlockPeriodisation(_),
                ) => None,
            };

            Ok(measured.and_then(|load| {
                Anchor::new(load, None, AnchorProvenance::Tested, span.end()).ok()
            }))
        })
    }

    /// What the record says a lift measured inside a span.
    ///
    /// The rule is [`heaviest_completed`]; this is the read that supplies it.
    async fn measured_in(
        &self,
        primary: RepsExercise,
        span: Span,
    ) -> Result<Option<Kg>, PrescriptionError> {
        let performances = self.ports.history.performances(primary).await?;
        Ok(heaviest_completed(&performances, span))
    }

    async fn sbs_maximum(
        &self,
        plan: &PlanName,
        sbs: &Sbs,
        parameters: &GenerationParameters,
        before: Date,
    ) -> Result<Kg, PrescriptionError> {
        let Some(opening) = self.opening_of(sbs, parameters).await? else {
            return Err(PrescriptionError::NoInheritedMaximum {
                start: sbs.calendar().start(),
            });
        };
        let mut maximum = opening.load();
        let Exercise::Reps(primary) = sbs.primary_exercise() else {
            return Ok(maximum);
        };
        // The increment is the plate grid's, not a number of this module's:
        // what SBS's `FLOOR` rounds to is whatever the bar can actually hold.
        let Some(steps) = parameters.scales.for_exercise(Exercise::Reps(primary)) else {
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
    /// answers the first question with nothing — which is exactly the case
    /// [`TestTarget::Declared`] exists for. It still answers the second: the
    /// light session is the predecessor's session whatever it was training.
    async fn inheritance(
        &self,
        programme: &Mesocycle,
        parameters: &GenerationParameters,
        date: Date,
    ) -> Result<Inheritance, PrescriptionError> {
        let Mesocycle::Test(test) = programme else {
            return Ok(Inheritance {
                target: None,
                light: None,
            });
        };

        let declared = match test.target() {
            TestTarget::Declared(load) => Some(load),
            TestTarget::Inherited => None,
        };

        let predecessor = self
            .ports
            .programmes
            .preceding(test.calendar().start())
            .await?;
        let Some((_, before_plan, Mesocycle::Progression(Progression::Linear(before)))) =
            predecessor
        else {
            // Nothing before it, or a predecessor with no ladder to read a
            // position off. A block's exit test anchors what follows through its
            // own result rather than through a target, so a test after one has
            // nothing to inherit either.
            return Ok(Inheritance {
                target: declared,
                light: None,
            });
        };

        let progress = self
            .progress(&before_plan, &before, parameters, date)
            .await?;
        let Ok(ladder) = before.ladder(parameters) else {
            return Ok(Inheritance {
                target: declared,
                light: None,
            });
        };
        let Ok(steps) = before.steps(parameters) else {
            return Ok(Inheritance {
                target: declared,
                light: None,
            });
        };

        let inherited = (before.primary_exercise() == test.primary_exercise())
            .then(|| progress.test_target(ladder, steps));
        Ok(Inheritance {
            // A declared target wins: it is the operator saying what this test
            // is for, and inheritance is the default rather than an override.
            target: declared.or(inherited),
            light: progress.light_top_set(ladder, steps, parameters.light_of_heavy),
        })
    }

    async fn progress(
        &self,
        plan: &PlanName,
        programme: &Linear,
        parameters: &GenerationParameters,
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
            parameters.first_reset,
            parameters.second_reset,
            programme.steps(parameters)?,
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
    /// template whose anchor does not move.
    maximum: Option<Kg>,
}

/// Every position the template issues, derived in order.
///
/// The order is [`PrimaryPattern::sequence`]'s; all this adds is which
/// derivation each position gets — the primary its top set and back-offs, and
/// everything else double progression, a hold, or its authored numbers.
fn issue_slots(
    programme: &Mesocycle,
    parameters: &GenerationParameters,
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
                primary_slot_item(programme, parameters, role, week, standing)
            }
            Position::Single(slot) => accessory_slot(programme, parameters, role, history, slot),
            Position::Superset(first, second) => {
                group(programme, parameters, role, history, first, second, &[])
            }
            Position::Circuit([first, second, third, fourth]) => group(
                programme,
                parameters,
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
const BLOCK_ENTRY_TEST_ROLE: SessionRole = SessionRole::Heavy;

/// Which week this session belongs to, in the vocabulary the store speaks.
///
/// **The calendar cannot answer for a block.** Since decision 0013 a calendar
/// emits nothing but climbing weeks — a linear programme has no test and a
/// block's entry test is not one of its weeks — so which week is a block's exit
/// test is decided by the phase plan and nothing else. A standalone test week is
/// a test week on both its sessions: the week is what it is, and which session
/// is the attempt is the role's business.
fn week_of(programme: &Mesocycle, placed: WeekKind) -> WeekKind {
    match programme {
        Mesocycle::Test(_) => WeekKind::Test,
        // A linear programme's weeks are all climbing weeks, and an SBS cycle's
        // are too: **week 4 is not a test week even though it ends on a test**,
        // because its first session is a taper the chart states in full. Calling
        // the week a test would send the light session looking for a predecessor
        // to inherit from, which an SBS cycle never needs.
        Mesocycle::Progression(Progression::Linear(_) | Progression::Provided { .. }) => placed,
        Mesocycle::Progression(Progression::BlockPeriodisation(block)) => {
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
    programme: &Mesocycle,
    inheritance: Inheritance,
    opening: Option<Anchor>,
) -> Result<DerivedFrom, PrescriptionError> {
    match programme {
        // **The resolved anchor, not the authored one.** A ladder and a block
        // state theirs and the two are the same value; a provided cycle that
        // inherits states none, and what it opens from is the maximum its
        // predecessor measured. Absent, the session is refused rather than
        // issued against nothing.
        Mesocycle::Progression(_) => {
            opening
                .map(DerivedFrom::Anchor)
                .ok_or_else(|| PrescriptionError::NoInheritedMaximum {
                    start: programme.calendar().start(),
                })
        }
        Mesocycle::Test(test) => {
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
    /// A ramp toward a load, then one autoregulated attempt at it.
    ///
    /// **Open at the top, always.** Going past the number is the outcome the
    /// week exists to produce, so nothing caps it: the target is what the ramp
    /// is built toward and what the report names, not a ceiling.
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
    programme: &Mesocycle,
    parameters: &GenerationParameters,
    role: SessionRole,
    week: WeekKind,
    standing: Standing,
) -> Derived {
    let Standing {
        progress,
        inheritance,
        maximum,
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

    let Some(steps) = parameters.scales.for_exercise(*exercise) else {
        return Derived::underivable(UnderivableSlot {
            slot,
            exercise: exercise.as_str(),
            reason: UnderivableReason::NoLoadScale,
        });
    };

    let plan = match programme {
        Mesocycle::Progression(Progression::Linear(linear)) => {
            linear_load(linear, parameters, role, week, progress, steps)
        }
        Mesocycle::Progression(Progression::BlockPeriodisation(block)) => {
            block_load(block, role, week, steps)
        }
        Mesocycle::Progression(Progression::Provided { cycle: sbs, .. }) => {
            sbs_load(sbs, role, week, steps, maximum)
        }
        Mesocycle::Test(test) => test_load(test, parameters, role, inheritance, steps),
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

    let sets = primary_sets(plan, parameters, role, steps);

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
    parameters: &GenerationParameters,
    role: SessionRole,
    week: WeekKind,
    progress: Option<Progress>,
    steps: &LoadSteps,
) -> Result<PrimaryLoad, UnderivableReason> {
    let (Some(progress), Ok(ladder)) = (progress, linear.ladder(parameters)) else {
        return Err(UnderivableReason::NoLadder);
    };
    let WeekKind::Climbing(_) = week else {
        return Err(UnderivableReason::NoLadder);
    };
    let load = match role {
        SessionRole::Heavy => progress.heavy_top_set(ladder, steps),
        SessionRole::Light => progress.light_top_set(ladder, steps, parameters.light_of_heavy),
    };
    load.map_or(Err(UnderivableReason::NoLadder), |load| {
        Ok(PrimaryLoad::TopSet {
            load,
            reps: parameters.top_set_reps.get(role).as_rep_count(),
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
    sbs: &Sbs,
    role: SessionRole,
    week: WeekKind,
    steps: &LoadSteps,
    maximum: Option<Kg>,
) -> Result<PrimaryLoad, UnderivableReason> {
    let WeekKind::Climbing(index) = week else {
        return Err(UnderivableReason::NoLadder);
    };
    let session = match role {
        SessionRole::Light => SbsSession::First,
        SessionRole::Heavy => SbsSession::Second,
    };
    let Ok(day) = sbs_day(index.as_u32(), session) else {
        return Err(UnderivableReason::NoLadder);
    };

    // **What the record says the cycle is at**, falling back to the opening
    // anchor only when nothing has been read — which is week one, and a cycle
    // whose primary has no load scale. A cycle that inherits has no authored
    // number to fall back to: resolving it needs the store, which is the
    // caller's business, so an unresolved one is underivable here rather than
    // opened from a guess.
    let Some(maximum) = maximum.or_else(|| sbs.entry().anchor().map(Anchor::load)) else {
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
    let anchor = block.entry().anchor().load();
    let planned = match planned {
        // **The week the block measures what it is about to plan from.** The ramp
        // builds toward the anchor the block was authored with, expressed at the
        // repetition count the attempt is performed at — so a triple works up to
        // the 3RM the operator expects rather than to a one-rep maximum nobody
        // is attempting. Nothing here reads another programme: what this block
        // expects is the block's own statement, and the week is where it finds
        // out whether it was right.
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
    parameters: &GenerationParameters,
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
        reps: parameters.top_set_reps.get(role).as_rep_count(),
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
    programme: &Mesocycle,
    parameters: &GenerationParameters,
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
                accessory_exercise(programme, parameters, role, history, slot),
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
    programme: &Mesocycle,
    parameters: &GenerationParameters,
    role: SessionRole,
    history: &BTreeMap<RepsExercise, LastPerformance>,
    slot: SlotId,
) -> Derived {
    match accessory_exercise(programme, parameters, role, history, slot) {
        Ok(exercise) => Derived::item(PrescribedItem::Exercise { slot, exercise }),
        Err(reason) => Derived::underivable(reason),
    }
}

/// What a non-primary slot prescribes, before it is placed in an item.
///
/// Separate from [`accessory_slot`] because a supersetted position needs the
/// exercise without the item wrapped around it.
fn accessory_exercise(
    programme: &Mesocycle,
    parameters: &GenerationParameters,
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
        SlotContent::Single(exercise) => one_exercise(parameters, history, *exercise, slot),
    }
}

/// The scheme a block's non-primary slots run.
const fn scheme_for(
    parameters: &GenerationParameters,
    block: Block,
) -> &domain::prescription::AccessoryScheme {
    match block {
        Block::Hypertrophy => &parameters.hypertrophy,
        _ => &parameters.strength,
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
fn hold(parameters: &GenerationParameters, exercise: DurationExercise) -> PrescribedExercise {
    let set = || {
        PrescribedSet::fixed(
            // Unloaded, and the pinned axis is volume rather than intensity —
            // which is how a slot with no load still prescribes something.
            Load::UNLOADED,
            Target::Exactly(parameters.static_hold),
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
    parameters: &GenerationParameters,
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
        return Ok(hold(parameters, duration_exercise));
    }

    let Exercise::Reps(reps_exercise) = exercise else {
        return Err(underivable(UnderivableReason::NotCountedInReps));
    };
    let Some(LastPerformance::Performed(last)) = history.get(&reps_exercise) else {
        return Err(underivable(UnderivableReason::NeverPerformed));
    };
    let scheme = scheme_for(parameters, slot.block());
    let load = progressed_load(parameters, exercise, scheme, last).map_err(underivable)?;
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
    parameters: &GenerationParameters,
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

    let steps = parameters
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

/// Storing an authored plan and the parameters it was generated against.
pub struct Authoring<P, M, G> {
    plans: P,
    mesocycles: M,
    parameters: G,
}

impl<P, M, G> Authoring<P, M, G> {
    pub const fn new(plans: P, mesocycles: M, parameters: G) -> Self {
        Self {
            plans,
            mesocycles,
            parameters,
        }
    }
}

impl<P, M, G> Authoring<P, M, G>
where
    P: Sync,
    M: MesocycleStore + Sync,
    G: Sync,
{
    /// Whether a claimed earlier maximum is one that exists.
    ///
    /// **A block's anchor comes from one of three places** (decision 0016): a
    /// previous test, an entry test of its own, or a declared number. Only the
    /// first says something about the past, so only the first is checked here —
    /// and it can only be checked here, because whether such a test happened is a
    /// fact about the store rather than a claim the plan can settle about
    /// itself.
    ///
    /// The operator's ten compositions are what a claimed test has to survive:
    ///
    /// ```text
    /// test a   → block b   produces a ≠ b            no such test
    /// test b   → block b   produces b                opens from it
    /// linear a → block b   produces nothing          no such test
    /// linear b → block b   produces nothing          no such test
    /// block a  → block b   produces a ≠ b            no such test
    /// block b  → block b   produces b, its exit      opens from it
    ///
    /// test b  → block b                adjacent      opens from it
    /// test b  → 1 blank week → block b               opens from it
    /// test b  → 2 blank weeks → block b              too old
    /// block b → 1 blank week → block b               opens from it
    /// block b → 2 blank weeks → block b              too old
    /// ```
    ///
    /// Row four is the one that invites a wrong guess, and it is why the
    /// predicate is worth having: a linear programme for the *same* lift still
    /// leaves no maximum, because it never tests. Its last heavy single feels
    /// like one and is not — and `provenance = "tested"` beside that date is
    /// exactly what this refuses.
    ///
    /// **It refuses a claim, not a choice.** A block that has no test to inherit
    /// is free to run its own entry test or to declare a number; what it may not
    /// do is say a measurement happened when none did.
    ///
    /// **The predecessor is usually the previous element of a list.** Inside a
    /// plan the mesocycle before this one is known without asking anything, and
    /// only the mesocycle that *opens* the plan has to put the question to the
    /// store — where the answer belongs to the plan that ran before.
    ///
    /// # Errors
    ///
    /// [`PrescriptionError`] if the store is unavailable, if no test of this lift
    /// ran before this block, if the anchor is not dated to it, or if it is too
    /// old to still speak.
    async fn claimed_maximum_exists(
        &self,
        plan: &PlanName,
        preceding: Option<&Mesocycle>,
        programme: &Mesocycle,
    ) -> Result<(), PrescriptionError> {
        if !programme.claims_an_earlier_maximum() {
            return Ok(());
        }
        let start = programme.calendar().start();
        let wanted = programme.primary_exercise();
        let Some(anchor) = programme.anchor() else {
            return Ok(());
        };

        let found = match preceding {
            Some(before) => Some(before.clone()),
            None => self
                .mesocycles
                .preceding(start)
                .await?
                .map(|(_, _, before)| before),
        };
        let before = match found {
            Some(before) if before.produces_maximum() == Some(wanted) => before,
            found => {
                return Err(PrescriptionError::NoMaximumToOpenFrom {
                    plan: plan.clone(),
                    start,
                    primary: wanted.as_str(),
                    predecessor: found.map(|before| before.calendar().start()),
                });
            }
        };

        // **Dated to that test, and recent.** Either alone lets a number in from
        // nowhere: a date inside the predecessor with no bound on age would
        // accept a maximum from a block that finished in June, and a recent date
        // with no bound on origin would accept one written down last week.
        if !before.span().covers(anchor.from()) {
            return Err(PrescriptionError::MaximumIsNotTheOneBefore {
                plan: plan.clone(),
                start,
                tested: anchor.from(),
                predecessor: before.calendar().start(),
            });
        }
        if !is_recent_enough(anchor.from(), start) {
            return Err(PrescriptionError::MaximumIsStale {
                plan: plan.clone(),
                tested: anchor.from(),
                start,
                weeks: RECENT_WEEKS,
            });
        }
        Ok(())
    }

    /// Every gym mesocycle of a plan, each checked against what precedes it.
    async fn claims_are_sound(&self, plan: &Plan) -> Result<(), PrescriptionError> {
        let Some(gym) = plan.gym() else {
            return Ok(());
        };
        let mesocycles: Vec<&Mesocycle> = gym.mesocycles().collect();
        for (at, mesocycle) in mesocycles.iter().enumerate() {
            let preceding = at.checked_sub(1).and_then(|before| mesocycles.get(before));
            self.claimed_maximum_exists(plan.name(), preceding.copied(), mesocycle)
                .await?;
        }
        Ok(())
    }
}

impl<P, M, G> PlanAuthor for Authoring<P, M, G>
where
    P: PlanStore + Sync,
    M: MesocycleStore + Sync,
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

        // **And a block claiming to open from an earlier test has to be right
        // about that** (decision 0016). This is the only rule in the system that
        // reads another mesocycle in order to refuse this one, and it has to:
        // whether a measurement happened is a fact about what came before, not
        // something the plan can settle about itself.
        self.claims_are_sound(plan).await?;

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
    parameters: &GenerationParameters,
    role: SessionRole,
    steps: &LoadSteps,
) -> Vec<PrescribedSet<RepCount>> {
    let mut sets: Vec<PrescribedSet<RepCount>> = Vec::new();
    // The ramp is a share of what the session is working toward, whatever that
    // is — never of the anchor. Ramping off the anchor had the operator warming
    // up toward a number they had passed three weeks earlier (decision 0011).
    let toward = match plan {
        PrimaryLoad::TopSet { load, .. } | PrimaryLoad::Across { load, .. } => load,
        PrimaryLoad::Attempt { toward, .. } | PrimaryLoad::RepMax { toward, .. } => toward,
    };
    // **The ramp's repetition counts follow the top set's** (decision 0030), but
    // only where the top set is maximal: a percentage day states a submaximal
    // load, so its ramp has nothing to rehearse. The stored ramp is the floor in
    // both cases, so a low count leaves it exactly as authored.
    let ramp = match plan {
        PrimaryLoad::RepMax { reps, .. } | PrimaryLoad::Attempt { reps, .. } => {
            Cow::Owned(warmup_ramp(&parameters.warmup, reps))
        }
        PrimaryLoad::TopSet { .. } | PrimaryLoad::Across { .. } => {
            Cow::Borrowed(&parameters.warmup)
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
            let pattern = parameters.back_off.get(role);
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
            // **The target is in the plan, so it is printed.** The ramp above is
            // built as a share of exactly this number, so omitting it left the
            // operator working up to something the session had already decided
            // and would not say.
            //
            // Decision 0011 keeps the target out of `programme show`, and that
            // still holds: there it is a *projection* for a week that has not
            // happened, and every session between now and then can move it. A
            // prescription is issued for one date against the record as it
            // stands, which is the moment the number is knowable.
            //
            // Nothing caps it. Going past is the outcome the day exists to
            // produce, and zero in reserve is what says so.
            sets.push(
                PrescribedSet::fixed(Load::Absolute(toward), Target::Exactly(reps))
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
        gym::{Kg, Load, Performed, SetKind, SignedKg, exercise::Exercise},
        landing::LandingRecordId,
        measure::RepCount,
        prescription::seed::seed,
    };
    use jiff::civil::Date;

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
        let last = performance(vec![
            set(Load::absolute(Kg::from_grams(40_000)), 6).expect("a set"),
            set(Load::absolute(Kg::from_grams(30_000)), 6).expect("a set"),
            set(Load::absolute(Kg::from_grams(30_000)), 6).expect("a set"),
        ])
        .expect("a performance");

        let progressed = progressed_load(
            &parameters,
            Exercise::Reps(domain::gym::exercise::RepsExercise::PreacherCurlBarbell),
            &parameters.strength,
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
        let last = performance(vec![
            set(Load::relative(SignedKg::from_grams(-7_000)), 4).expect("a set"),
            set(Load::relative(SignedKg::from_grams(-14_000)), 5).expect("a set"),
            set(Load::relative(SignedKg::from_grams(-21_000)), 6).expect("a set"),
        ])
        .expect("a performance");

        let progressed = progressed_load(
            &parameters,
            Exercise::Reps(domain::gym::exercise::RepsExercise::ChestDip),
            &parameters.strength,
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
        let last = performance(vec![
            set(Load::absolute(Kg::from_grams(40_000)), 6).expect("a set"),
            set(Load::absolute(Kg::from_grams(30_000)), 4).expect("a set"),
        ])
        .expect("a performance");

        let progressed = progressed_load(
            &parameters,
            Exercise::Reps(domain::gym::exercise::RepsExercise::PreacherCurlBarbell),
            &parameters.strength,
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
        gym::{Kg, Load, Performed, SetKind},
        landing::LandingRecordId,
        measure::RepCount,
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

    use super::{PrimaryLoad, primary_sets};
    use domain::{
        gym::Kg,
        measure::RepCount,
        prescription::{LoadSteps, SessionRole, seed::seed},
    };

    fn reps_of(plan: PrimaryLoad) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
        let parameters = seed()?;
        let steps = LoadSteps::uniform(Kg::from_grams(2_500))?;
        let sets = primary_sets(plan, &parameters, SessionRole::Heavy, &steps);
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
