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
        Block, GatingTopSet, GenerationParameters, Position, PrescribedExercise, PrescribedItem,
        PrescribedSet, PrescribedSuperset, PrescribedWorkout, Programme, ProgrammeId, Progress,
        SessionRole, SlotId, SupersetMember, Target, WeekKind, WorkoutShape, linear::SlotContent,
        progress_after,
    },
};
use jiff::{Timestamp, civil::Date};

use crate::{
    error::PrescriptionError,
    ports::{
        ExerciseHistory, GenerationParameterStore, LadderStanding, LastPerformance, Performance,
        PrescribedWorkoutStore, Prescription, ProgrammeAuthor, ProgrammeStore, Reissue,
        UnderivableReason, UnderivableSlot, WorkoutPrescriber,
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
        let Some((programme_id, programme)) = self.ports.programmes.current().await? else {
            return Err(PrescriptionError::NoProgramme);
        };
        let Some((_, parameters)) = self.ports.parameters.current().await? else {
            return Err(PrescriptionError::NoParameters);
        };
        let progress = self.progress(&programme, &parameters, on).await?;
        Ok(LadderStanding {
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

        let Some((programme_id, programme)) = self.ports.programmes.current().await? else {
            return Err(PrescriptionError::NoProgramme);
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
        let progress = self.progress(&programme, &parameters, date).await?;

        let mut items = Vec::new();
        let mut underivable = Vec::new();
        for derived in issue_slots(&programme, &parameters, role, week, progress, &history) {
            match derived {
                Derived::Item(item) => items.push(*item),
                Derived::Underivable(slots) => underivable.extend(slots),
            }
        }

        let items = NonEmpty::new(items).map_err(|_| PrescriptionError::NothingDerivable)?;
        let workout = PrescribedWorkout::new(
            WorkoutShape::new(items),
            date,
            role,
            week,
            programme.anchor(),
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
    async fn progress(
        &self,
        programme: &Programme,
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
    progress: Progress,
    history: &BTreeMap<RepsExercise, LastPerformance>,
) -> Vec<Derived> {
    let primary = programme.primary();
    primary
        .sequence()
        .into_iter()
        .map(|position| match position {
            Position::Single(slot) if slot == primary.slot() => {
                primary_slot_item(programme, parameters, role, week, progress)
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

/// The primary slot: a warm-up ramp, a top set, and back-offs.
///
/// On a test week the top set is `Autoregulated` — load open, one repetition,
/// nothing in reserve — which is exactly what a test is and exactly what that
/// variant was for. It had been recorded as reachable but unreached; the test
/// week is what reaches it.
fn primary_slot_item(
    programme: &Programme,
    parameters: &GenerationParameters,
    role: SessionRole,
    week: WeekKind,
    progress: Progress,
) -> Derived {
    let slot = programme.primary().slot();
    let exercise = programme.fills().primary(programme.primary(), role);
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
    let mut sets: Vec<PrescribedSet<RepCount>> = Vec::new();

    let top_set = match week {
        // **The calendar says whether, and the record says which.** A climbing week
        // takes its load from where the progression has got to, not from the week
        // the date falls in — those agree until the first miss and diverge after
        // it, which is the whole of US3.
        WeekKind::Climbing(_) => {
            let Ok(ladder) = programme.ladder(parameters) else {
                return Derived::underivable(UnderivableSlot {
                    slot,
                    exercise: exercise.as_str(),
                    reason: UnderivableReason::NoLadder,
                });
            };
            match role {
                SessionRole::Heavy => progress.heavy_top_set(ladder, steps),
                SessionRole::Light => {
                    progress.light_top_set(ladder, steps, parameters.light_of_heavy)
                }
            }
        }
        // A test has no ladder position; its load is what the day allows.
        WeekKind::Test => None,
    };

    if let Some(load) = top_set {
        // The ramp, then the top set, then the back-offs. Every step is a
        // percentage of the top set and never of the anchor.
        for step in parameters.warmup.iter() {
            sets.push(PrescribedSet::warmup(
                Load::Absolute(steps.quantise_loaded(step.of_top_set.of(load))),
                Target::Exactly(step.reps),
            ));
        }
        let reps = parameters.top_set_reps.get(role).as_rep_count();
        sets.push(PrescribedSet::fixed(
            Load::Absolute(load),
            Target::Exactly(reps),
        ));

        // The back-offs are the role's own pattern — heavy `2 × 4`, light
        // `3 × 6` — and not the strength block's accessory scheme. Borrowing
        // that scheme is what issued the light session's three sets of six on
        // the heavy day; the operator stated the two patterns on 2026-08-20 and
        // the record agrees on every session since the July test.
        let pattern = parameters.back_off.get(role);
        let back_off = steps.quantise_loaded(pattern.of_top_set.of(load));
        for _ in 0..pattern.sets.as_u32() {
            sets.push(PrescribedSet::fixed(
                Load::Absolute(back_off),
                Target::Exactly(pattern.reps),
            ));
        }
    } else {
        // A test week. Ramp against the anchor, since there is no top set to take
        // a percentage of, then work up.
        let anchor = programme.anchor().load();
        for step in parameters.warmup.iter() {
            sets.push(PrescribedSet::warmup(
                Load::Absolute(steps.quantise_loaded(step.of_top_set.of(anchor))),
                Target::Exactly(step.reps),
            ));
        }
        let Ok(single) = RepCount::new(1) else {
            return Derived::underivable(UnderivableSlot {
                slot,
                exercise: exercise.as_str(),
                reason: UnderivableReason::NoLadder,
            });
        };
        sets.push(PrescribedSet::autoregulated(
            Target::Exactly(single),
            domain::gym::Rir::Zero,
        ));
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
    let target = Target::range(scheme.low, scheme.high)
        .map_err(|_| underivable(UnderivableReason::NoWorkingSet))?;
    let sets: Vec<_> = (0..scheme.sets.as_u32())
        .map(|_| PrescribedSet::fixed(load, target))
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
            .is_some_and(|reps| *reps >= scheme.high)
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

impl<P, G> ProgrammeAuthor for Authoring<P, G>
where
    P: ProgrammeStore + Sync,
    G: GenerationParameterStore + Sync,
{
    async fn author(
        &self,
        programme: &Programme,
        parameters: &GenerationParameters,
    ) -> Result<ProgrammeId, PrescriptionError> {
        // Parameters first: a programme names the version it was authored
        // against, and one stored without them would reference nothing.
        self.parameters
            .author(programme.authored_at(), parameters)
            .await?;
        Ok(self.programmes.author(programme).await?)
    }
}
