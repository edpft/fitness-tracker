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
        Block, GatingTopSet, GenerationParameters, PrescribedExercise, PrescribedItem,
        PrescribedSet, PrescribedSuperset, PrescribedWorkout, Programme, ProgrammeId, Progress,
        SessionRole, SlotId, SupersetMember, Target, WeekKind, WorkoutShape, linear::SlotContent,
        progress_after, quantise_loaded,
    },
};
use jiff::{Timestamp, civil::Date};

use crate::{
    error::PrescriptionError,
    ports::{
        ExerciseHistory, GenerationParameterStore, LadderStanding, LastPerformance, Performance,
        PrescribedWorkoutStore, Prescription, ProgrammeAuthor, ProgrammeStore, UnderivableReason,
        UnderivableSlot, WorkoutPrescriber,
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
    Underivable(UnderivableSlot),
}

impl Derived {
    fn item(item: PrescribedItem) -> Self {
        Self::Item(Box::new(item))
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

    async fn prescribe(&self, date: Date) -> Result<Prescription, PrescriptionError> {
        // Read what was issued before doing any work. Asking twice for one date
        // returns what was already issued rather than a second prescription, and
        // the derived ladder position means there is no counter that could have
        // advanced in between either.
        if let Some((id, workout)) = self.ports.prescriptions.issued_for(date).await? {
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
                Derived::Underivable(slot) => underivable.push(slot),
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
            programme.anchor().failed(),
            parameters.first_reset,
            parameters.second_reset,
            parameters.plate_increment,
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

/// Every slot, in issue order.
///
/// The strength block issues the primary first, then the upper pair supersetted,
/// then the remaining lower slot as the accessory — which is what the record
/// shows and what the model says. Where the primary is itself one of the upper
/// pair, the pair is not supersetted, because a slot cannot be both the session's
/// centrepiece and half of a paired accessory.
fn issue_slots(
    programme: &Programme,
    parameters: &GenerationParameters,
    role: SessionRole,
    week: WeekKind,
    progress: Progress,
    history: &BTreeMap<RepsExercise, LastPerformance>,
) -> Vec<Derived> {
    let mut issued = Vec::new();
    let accessory = |slot: SlotId| accessory_slot(programme, parameters, role, history, slot);

    issued.push(accessory(SlotId::Plyometric));
    issued.push(accessory(SlotId::Power));

    // The primary, then the upper pair, then the accessory lower slot.
    let primary_slot = programme.primary().slot();
    issued.push(primary_slot_item(
        programme,
        parameters,
        role,
        week,
        progress,
        primary_slot,
    ));

    let upper = [SlotId::UpperPush, SlotId::UpperPull];
    if upper.contains(&primary_slot) {
        // One of the pair is the primary, so the other stands alone.
        for slot in upper.into_iter().filter(|slot| *slot != primary_slot) {
            issued.push(accessory(slot));
        }
    } else {
        issued.push(pair(programme, parameters, role, history, upper));
    }

    let lower = [SlotId::KneeDominant, SlotId::HipDominant];
    for slot in lower.into_iter().filter(|slot| *slot != primary_slot) {
        issued.push(accessory(slot));
    }

    issued.push(accessory(SlotId::Arms));
    issued.push(accessory(SlotId::Forearms));
    issued.push(accessory(SlotId::Core));
    issued.push(accessory(SlotId::MobilityHold));
    issued.push(accessory(SlotId::MobilityStretch));

    issued
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
    slot: SlotId,
) -> Derived {
    let SlotContent::Single(exercise) = programme.fills().content(slot, role) else {
        // Unreachable: the four strength slots are all single.
        return Derived::Underivable(UnderivableSlot {
            slot,
            exercise: "",
            reason: UnderivableReason::NotSingle,
        });
    };
    let Exercise::Reps(reps_exercise) = exercise else {
        return Derived::Underivable(UnderivableSlot {
            slot,
            exercise: exercise.as_str(),
            reason: UnderivableReason::NotCountedInReps,
        });
    };

    let increment = parameters.plate_increment;
    let mut sets: Vec<PrescribedSet<RepCount>> = Vec::new();

    let top_set = match week {
        // **The calendar says whether, and the record says which.** A climbing week
        // takes its load from where the progression has got to, not from the week
        // the date falls in — those agree until the first miss and diverge after
        // it, which is the whole of US3.
        WeekKind::Climbing(_) => {
            let Ok(ladder) = programme.ladder(parameters) else {
                return Derived::Underivable(UnderivableSlot {
                    slot,
                    exercise: exercise.as_str(),
                    reason: UnderivableReason::NoLadder,
                });
            };
            match role {
                SessionRole::Heavy => progress.heavy_top_set(ladder, increment),
                SessionRole::Light => {
                    progress.light_top_set(ladder, increment, parameters.light_of_heavy)
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
                Load::Absolute(quantise_loaded(step.of_top_set.of(load), increment)),
                Target::Exactly(step.reps),
            ));
        }
        let reps = parameters.top_set_reps.get(role).as_rep_count();
        sets.push(PrescribedSet::fixed(
            Load::Absolute(load),
            Target::Exactly(reps),
        ));

        // The back-offs run the strength block's scheme, the primary being a
        // strength slot. Their own sets and repetitions are not separately
        // authored — the record shows the two roles differing, which is a
        // distinction nobody has stated.
        let back_off = quantise_loaded(parameters.back_off_of_top_set.of(load), increment);
        let scheme = scheme_for(parameters, Block::Strength);
        let back_off_reps = scheme.high;
        for _ in 0..scheme.sets.as_u32() {
            sets.push(PrescribedSet::fixed(
                Load::Absolute(back_off),
                Target::Exactly(back_off_reps),
            ));
        }
    } else {
        // A test week. Ramp against the anchor, since there is no top set to take
        // a percentage of, then work up.
        let anchor = programme.anchor().load();
        for step in parameters.warmup.iter() {
            sets.push(PrescribedSet::warmup(
                Load::Absolute(quantise_loaded(step.of_top_set.of(anchor), increment)),
                Target::Exactly(step.reps),
            ));
        }
        let Ok(single) = RepCount::new(1) else {
            return Derived::Underivable(UnderivableSlot {
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
        return Derived::Underivable(UnderivableSlot {
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

/// Two slots issued back to back.
fn pair(
    programme: &Programme,
    parameters: &GenerationParameters,
    role: SessionRole,
    history: &BTreeMap<RepsExercise, LastPerformance>,
    slots: [SlotId; 2],
) -> Derived {
    let mut members = Vec::new();
    for slot in slots {
        match accessory_slot(programme, parameters, role, history, slot) {
            Derived::Item(item) => match *item {
                PrescribedItem::Exercise { slot, exercise } => {
                    members.push(SupersetMember { slot, exercise });
                }
                // A single slot never derives to a superset: `accessory_slot`
                // returns an `Exercise` item for a `SlotContent::Single`, and
                // both halves of the upper pair are single by the template.
                PrescribedItem::Superset(_) => return Derived::Item(item),
            },
            // Either member failing costs the pair. Issuing one half of a
            // supersetted pair would be prescribing something the template does
            // not describe.
            Derived::Underivable(reason) => return Derived::Underivable(reason),
        }
    }

    let mut members = members.into_iter();
    let (Some(first), Some(second)) = (members.next(), members.next()) else {
        return Derived::Underivable(UnderivableSlot {
            slot: slots[0],
            exercise: "",
            reason: UnderivableReason::NoWorkingSet,
        });
    };
    Derived::item(PrescribedItem::Superset(PrescribedSuperset {
        members: AtLeastTwo::of(first, second, Vec::new()),
    }))
}

/// Any slot that is not the primary: double progression, or static.
fn accessory_slot(
    programme: &Programme,
    parameters: &GenerationParameters,
    role: SessionRole,
    history: &BTreeMap<RepsExercise, LastPerformance>,
    slot: SlotId,
) -> Derived {
    match programme.fills().content(slot, role) {
        // Authored outright: no history is read and none is needed.
        SlotContent::Static(fill) => match static_exercise(fill) {
            Ok(exercise) => Derived::item(PrescribedItem::Exercise { slot, exercise }),
            Err(reason) => Derived::Underivable(UnderivableSlot {
                slot,
                exercise: fill.exercise.as_str(),
                reason,
            }),
        },
        SlotContent::Single(exercise) => match one_exercise(parameters, history, *exercise, slot) {
            Ok(exercise) => Derived::item(PrescribedItem::Exercise { slot, exercise }),
            Err(reason) => Derived::Underivable(reason),
        },
        SlotContent::Superset(members) => {
            let mut built = Vec::new();
            for exercise in members.iter() {
                match one_exercise(parameters, history, *exercise, slot) {
                    Ok(prescribed) => built.push(SupersetMember {
                        slot,
                        exercise: prescribed,
                    }),
                    Err(reason) => return Derived::Underivable(reason),
                }
            }
            let mut built = built.into_iter();
            let (Some(first), Some(second)) = (built.next(), built.next()) else {
                return Derived::Underivable(UnderivableSlot {
                    slot,
                    exercise: "",
                    reason: UnderivableReason::NoWorkingSet,
                });
            };
            Derived::item(PrescribedItem::Superset(PrescribedSuperset {
                members: AtLeastTwo::of(first, second, built.collect()),
            }))
        }
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
    let load = progressed_load(parameters, scheme, last)
        .ok_or_else(|| underivable(UnderivableReason::NoWorkingSet))?;
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
fn progressed_load(
    parameters: &GenerationParameters,
    scheme: &domain::prescription::AccessoryScheme,
    last: &Performance,
) -> Option<Load> {
    let heaviest = last.sets.last()?;
    let reached_top = last.sets.iter().all(|set| {
        set.outcome
            .completed()
            .is_some_and(|reps| *reps >= scheme.high)
    });

    if !reached_top {
        return Some(heaviest.load);
    }
    match heaviest.load {
        Load::Absolute(mass) => Some(Load::Absolute(Kg::from_grams(
            mass.as_grams() + parameters.plate_increment.as_kg().as_grams(),
        ))),
        // A relative load progresses the same way, on the axis it runs on.
        Load::Relative(delta) => Some(Load::Relative(domain::gym::SignedKg::from_grams(
            delta.as_grams()
                + i64::try_from(parameters.plate_increment.as_kg().as_grams()).unwrap_or(0),
        ))),
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
