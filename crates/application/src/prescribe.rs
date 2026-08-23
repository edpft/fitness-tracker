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

use std::collections::BTreeMap;

use domain::{
    gym::{
        Kg, Load, NonEmpty, RepCount,
        exercise::{DurationExercise, Exercise, RepsExercise},
        sequence::AtLeastTwo,
    },
    prescription::{
        Block, BlockWeek, DerivedFrom, GatingTopSet, GenerationParameters, Linear, LoadSteps,
        Periodisation, Periodised, Position, PrescribedExercise, PrescribedItem, PrescribedSet,
        PrescribedSuperset, PrescribedWorkout, Programme, ProgrammeId, Progress, RECENT_WEEKS,
        SessionRole, SlotId, SupersetMember, Target, Test, TestTarget, WeekKind, WeekPlan,
        WorkoutShape, is_recent_enough, linear::SlotContent, progress_after, rep_max, rested,
    },
};
use jiff::{Timestamp, civil::Date};

use crate::{
    error::PrescriptionError,
    ports::{
        Authored, ExerciseHistory, GenerationParameterStore, LadderStanding, LastPerformance,
        Performance, PrescribedWorkoutStore, Prescription, ProgrammeAuthor, ProgrammeStore,
        Reissue, UnderivableReason, UnderivableSlot, WorkoutPrescriber,
    },
};

/// Everything generation needs from the outside.
pub struct PrescriptionPorts<H, P, G, S> {
    pub history: H,
    pub programmes: P,
    pub parameters: G,
    pub prescriptions: S,
}

/// The use case.
pub struct Prescribing<H, P, G, S> {
    ports: PrescriptionPorts<H, P, G, S>,
}

impl<H, P, G, S> Prescribing<H, P, G, S> {
    pub const fn new(ports: PrescriptionPorts<H, P, G, S>) -> Self {
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

impl<H, P, G, S> WorkoutPrescriber for Prescribing<H, P, G, S>
where
    H: ExerciseHistory + Sync,
    P: ProgrammeStore + Sync,
    G: GenerationParameterStore + Sync,
    S: PrescribedWorkoutStore + Sync,
{
    async fn standing(&self, on: Date) -> Result<LadderStanding, PrescriptionError> {
        let Some((programme_id, programme)) = self.ports.programmes.on(on).await? else {
            return Err(PrescriptionError::NoProgramme { date: on });
        };
        let Some((_, parameters)) = self.ports.parameters.current().await? else {
            return Err(PrescriptionError::NoParameters);
        };
        let progress = self.progress_of(&programme, &parameters, on).await?;
        let target = self.inheritance(&programme, &parameters, on).await?.target;
        Ok(LadderStanding {
            target,
            programme_id,
            programme,
            parameters,
            progress,
            history_through: self.ports.history.newest_performance().await?,
        })
    }

    async fn prescribe(
        &self,
        date: Date,
        reissue: Reissue,
    ) -> Result<Prescription, PrescriptionError> {
        // Read what was issued before doing any work. Asking twice for one date
        // returns what was already issued rather than a second prescription, and
        // the derived ladder position means there is no counter that could have
        // advanced in between either.
        //
        // **Unless reissuing was asked for**, which is the answer to a
        // prescription derived from a programme that has since been corrected.
        // It is asked for rather than inferred: "the programme changed" cannot
        // be told from "the parameters changed" or from nothing having changed,
        // and silently re-deriving a session the operator may already be
        // halfway through is worse than making them say so.
        if reissue == Reissue::No
            && let Some((id, workout)) = self.ports.prescriptions.issued_for(date).await?
        {
            return Ok(Prescription {
                id,
                workout,
                freshly_issued: false,
                history_through: self.ports.history.newest_performance().await?,
                underivable: Vec::new(),
            });
        }

        let Some((programme_id, programme)) = self.ports.programmes.on(date).await? else {
            return Err(PrescriptionError::NoProgramme { date });
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
        let progress = self.progress_of(&programme, &parameters, date).await?;

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

        let mut items = Vec::new();
        let mut underivable = Vec::new();
        for derived in issue_slots(
            &programme,
            &parameters,
            role,
            week,
            progress,
            inheritance,
            &history,
        ) {
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
            derived_from(&programme, inheritance)?,
            parameters,
            parameters_at,
            programme_id,
            Timestamp::now(),
        );

        let id = self.ports.prescriptions.issue(&workout).await?;
        let history_through = self.ports.history.newest_performance().await?;

        Ok(Prescription {
            id,
            workout,
            freshly_issued: true,
            history_through,
            underivable,
        })
    }
}

impl<H, P, G, S> Prescribing<H, P, G, S>
where
    H: ExerciseHistory + Sync,
    P: ProgrammeStore + Sync,
    G: GenerationParameterStore + Sync,
    S: PrescribedWorkoutStore + Sync,
{
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
        programme: &Programme,
        parameters: &GenerationParameters,
        before: Date,
    ) -> Result<Option<Progress>, PrescriptionError> {
        match programme {
            Programme::Periodisation(Periodisation::Linear(linear)) => {
                Ok(Some(self.progress(linear, parameters, before).await?))
            }
            Programme::Periodisation(Periodisation::Block(_)) | Programme::Test(_) => Ok(None),
        }
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
        programme: &Programme,
        parameters: &GenerationParameters,
        date: Date,
    ) -> Result<Inheritance, PrescriptionError> {
        let Programme::Test(test) = programme else {
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
        let Some((_, Programme::Periodisation(Periodisation::Linear(before)))) = predecessor else {
            // Nothing before it, or a predecessor with no ladder to read a
            // position off. A block's exit test anchors what follows through its
            // own result rather than through a target, so a test after one has
            // nothing to inherit either.
            return Ok(Inheritance {
                target: declared,
                light: None,
            });
        };

        let progress = self.progress(&before, parameters, date).await?;
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
            let Ok((_, role)) = programme.calendar().place(performance.on) else {
                continue;
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

/// A gating session's top set: the heaviest working set, and what became of it.
///
/// **Heaviest rather than first.** In a session issued from this template the top
/// set is the first working set and the back-offs are lighter, so the two agree;
/// in the hand-run record they do not always, because that block opened with heavy
/// bridging singles tagged as warm-ups. Taking the heaviest is right under both
/// readings, and the failed attempt this exists to notice is by construction the
/// heaviest thing attempted.
///
/// `None` where the session recorded no working set at all, which is a session
/// that says nothing about the ladder rather than a miss.
fn top_set_of(performance: &Performance) -> Option<GatingTopSet> {
    let mut heaviest: Option<(u64, &crate::ports::PerformedSetSummary)> = None;
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

/// Every position the template issues, derived in order.
///
/// The order is [`PrimaryPattern::sequence`]'s; all this adds is which
/// derivation each position gets — the primary its top set and back-offs, and
/// everything else double progression, a hold, or its authored numbers.
fn issue_slots(
    programme: &Programme,
    parameters: &GenerationParameters,
    role: SessionRole,
    week: WeekKind,
    progress: Option<Progress>,
    inheritance: Inheritance,
    history: &BTreeMap<RepsExercise, LastPerformance>,
) -> Vec<Derived> {
    let primary = programme.primary();
    primary
        .sequence()
        .into_iter()
        .map(|position| match position {
            Position::Single(slot) if slot == primary.slot() => {
                primary_slot_item(programme, parameters, role, week, progress, inheritance)
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
fn week_of(programme: &Programme, placed: WeekKind) -> WeekKind {
    match programme {
        Programme::Test(_) => WeekKind::Test,
        Programme::Periodisation(Periodisation::Linear(_)) => placed,
        Programme::Periodisation(Periodisation::Block(block)) => {
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
    programme: &Programme,
    inheritance: Inheritance,
) -> Result<DerivedFrom, PrescriptionError> {
    match programme {
        Programme::Periodisation(periodisation) => Ok(DerivedFrom::Anchor(periodisation.anchor())),
        Programme::Test(test) => {
            inheritance
                .target
                .map(DerivedFrom::Target)
                .ok_or_else(|| PrescriptionError::NoTarget {
                    programme: test.name().clone(),
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
}

/// The primary slot: a warm-up ramp, and then whatever this template asks for.
fn primary_slot_item(
    programme: &Programme,
    parameters: &GenerationParameters,
    role: SessionRole,
    week: WeekKind,
    progress: Option<Progress>,
    inheritance: Inheritance,
) -> Derived {
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
        Programme::Periodisation(Periodisation::Linear(linear)) => {
            linear_load(linear, parameters, role, week, progress, steps)
        }
        Programme::Periodisation(Periodisation::Block(block)) => {
            block_load(block, role, week, steps)
        }
        Programme::Test(test) => test_load(test, parameters, role, inheritance, steps),
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

    let mut sets: Vec<PrescribedSet<RepCount>> = Vec::new();
    // The ramp is a share of what the session is working toward, whatever that
    // is — never of the anchor. Ramping off the anchor had the operator warming
    // up toward a number they had passed three weeks earlier (decision 0011).
    let toward = match plan {
        PrimaryLoad::TopSet { load, .. } | PrimaryLoad::Across { load, .. } => load,
        PrimaryLoad::Attempt { toward, .. } => toward,
    };
    for step in parameters.warmup.iter() {
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
        PrimaryLoad::Attempt { reps, .. } => {
            sets.push(PrescribedSet::autoregulated(
                Target::Exactly(reps),
                domain::gym::Rir::Zero,
            ));
        }
    }

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
fn block_load(
    block: &Periodised,
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

/// A standalone test's week: the attempt on the heavy session, and the
/// predecessor's session on the light one.
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
    // Not the test: the week's other session is the predecessor's, run at the
    // load its progression stands at. A test with nothing before it has no such
    // load, which is why a test that runs a second session needs a predecessor
    // even where its target was declared.
    let Some(load) = inheritance.light else {
        return Err(UnderivableReason::NoPredecessor);
    };
    Ok(PrimaryLoad::TopSet {
        load: steps.quantise_loaded(load),
        reps: parameters.top_set_reps.get(role).as_rep_count(),
    })
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
    programme: &Programme,
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
    programme: &Programme,
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
    programme: &Programme,
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

/// A static hold: the authored duration, once.
const fn hold(parameters: &GenerationParameters, exercise: DurationExercise) -> PrescribedExercise {
    let set = PrescribedSet::fixed(
        // Unloaded, and the pinned axis is volume rather than intensity — which
        // is how a slot with no load still prescribes something.
        Load::UNLOADED,
        Target::Exactly(parameters.static_hold),
    );
    PrescribedExercise::ForDuration {
        exercise,
        sets: NonEmpty::of(set, Vec::new()),
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
    let heaviest = last.sets.last().ok_or(UnderivableReason::NoWorkingSet)?;
    let reached_top = last.sets.iter().all(|set| {
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

/// Storing an authored programme and its parameters.
pub struct Authoring<P, G> {
    programmes: P,
    parameters: G,
}

impl<P, G> Authoring<P, G> {
    pub const fn new(programmes: P, parameters: G) -> Self {
        Self {
            programmes,
            parameters,
        }
    }
}

impl<P, G> Authoring<P, G>
where
    P: ProgrammeStore + Sync,
    G: Sync,
{
    /// Whether a claimed earlier maximum is one that exists.
    ///
    /// **A block's anchor comes from one of three places** (decision 0016): a
    /// previous test, an entry test of its own, or a declared number. Only the
    /// first says something about the past, so only the first is checked here —
    /// and it can only be checked here, because whether such a test happened is a
    /// fact about the store rather than a claim the document can settle about
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
    /// # Errors
    ///
    /// [`PrescriptionError`] if the store is unavailable, if no test of this lift
    /// ran before this block, if the anchor is not dated to it, or if it is too
    /// old to still speak.
    async fn claimed_maximum_exists(&self, programme: &Programme) -> Result<(), PrescriptionError> {
        if !programme.claims_an_earlier_maximum() {
            return Ok(());
        }
        let start = programme.calendar().start();
        let wanted = programme.primary_exercise();
        let Some(anchor) = programme.anchor() else {
            return Ok(());
        };

        let before = match self.programmes.preceding(start).await? {
            Some((_, before)) if before.produces_maximum() == Some(wanted) => before,
            found => {
                return Err(PrescriptionError::NoMaximumToOpenFrom {
                    programme: programme.name().clone(),
                    primary: wanted.as_str(),
                    predecessor: found.map(|(_, before)| before.name().clone()),
                });
            }
        };

        // **Dated to that test, and recent.** Either alone lets a number in from
        // nowhere: a date inside the predecessor with no bound on age would
        // accept a maximum from a block that finished in June, and a recent date
        // with no bound on origin would accept one written down last week.
        if !before.window().covers(anchor.from()) {
            return Err(PrescriptionError::MaximumIsNotTheOneBefore {
                programme: programme.name().clone(),
                tested: anchor.from(),
                predecessor: before.name().clone(),
            });
        }
        if !is_recent_enough(anchor.from(), start) {
            return Err(PrescriptionError::MaximumIsStale {
                programme: programme.name().clone(),
                tested: anchor.from(),
                start,
                weeks: RECENT_WEEKS,
            });
        }
        Ok(())
    }
}

impl<P, G> ProgrammeAuthor for Authoring<P, G>
where
    P: ProgrammeStore + Sync,
    G: GenerationParameterStore + Sync,
{
    async fn author(
        &self,
        programme: &Programme,
        parameters: &GenerationParameters,
    ) -> Result<(ProgrammeId, Authored), PrescriptionError> {
        // Refused before anything is written. Two programmes covering one day
        // would make which of them answers depend on the order rows came back
        // in, which is the silent ambiguity § 12's discipline exists to stop.
        // Versions of one programme never conflict — `overlaps` knows that a
        // shared name means a re-authoring rather than a rival.
        let proposed = programme.window();
        let mut authored = Authored::Created;
        for existing in self.programmes.windows().await? {
            if existing.name() == proposed.name() {
                authored = Authored::Modified;
                continue;
            }
            if proposed.overlaps(&existing) {
                return Err(PrescriptionError::OverlappingProgramme { proposed, existing });
            }
        }

        // **And a block claiming to open from an earlier test has to be right
        // about that** (decision 0016). This is the only rule in the system that
        // reads another programme in order to refuse this one, and it has to:
        // whether a measurement happened is a fact about what came before, not
        // something the document can settle about itself.
        self.claimed_maximum_exists(programme).await?;

        // Parameters first: a programme names the version it was authored
        // against, and one stored without them would reference nothing.
        self.parameters
            .author(programme.authored_at(), parameters)
            .await?;
        Ok((self.programmes.author(programme).await?, authored))
    }
}
