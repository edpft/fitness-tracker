//! A performance in prescription's vocabulary, and what comparing the two says.
//!
//! **The round trip, and it is deliberately lossy.** A performed workout and a
//! prescribed one have the same structure — ordered items, groupings, exercises,
//! sets carrying a load and a measure — so a total function from the first to the
//! second exists. Writing it down buys two things: a divergence names itself in
//! the domain's own vocabulary rather than in a human reading two printouts, and
//! what a prescription *is* stays separate from what makes it *issued*. Research
//! D9 has the argument; what follows is what a reader of the code needs.
//!
//! **What comes out is a [`WorkoutShape`], never a `PrescribedWorkout`.** That is
//! § 11 held by construction (§ 24): a projected shape has no anchor, no week and
//! no programme, so it cannot be handed to the store and recorded as something
//! that was issued. Were the two the same type, the record could come to hold a
//! prescription reverse-engineered from the very performance it exists to be
//! compared against, and expectation against reality would be unrecoverable.
//!
//! **A canonical session, so the visit is the unit** (§ 5). A visit merged from
//! four sources is one sequence of items, and which source each field came from
//! is provenance rather than shape — so nothing here can see the seams, exactly
//! as it could not see the seams between a session's Hevy routines.
//!
//! **Every loss is a [`ProjectionGap`] rather than an invented value** (FR-035):
//!
//! - **A failed attempt carries no intended count.** The load was on the bar and
//!   the repetitions being attempted are recorded nowhere, so it projects to
//!   `ToEffort` — load pinned, measure open — and reports the gap. Nothing
//!   guesses the count, and the effort is `Rir::Zero` because that is what
//!   failing *is* rather than an observation promoted into an instruction.
//! - **Slot identity is not in the performed record.** A performed workout has
//!   items and no slots, so slots are assigned by position against the template's
//!   [`ISSUE_ORDER`]. An item the order has no slot left for is a
//!   [`ProjectionGap::SlotUnassignable`] and is left out of the shape — the
//!   honest alternative to labelling it with a slot nobody prescribed.
//! - **Observed effort is dropped.** A recorded RIR is what happened; a
//!   prescribed effort is guidance. A completed set projects with no effort at
//!   all rather than presenting an observation as an instruction.
//! - **A set the record states too little of is left out.** An instruction needs
//!   a load, a count where the set completed, and whether it was a warm-up; a
//!   canonical set may hold none of the three, and not one of them has a
//!   defensible default — a missing load is not zero, and a missing kind read as
//!   working is a warm-up compared against a working set. Those are the same
//!   three fields `ExerciseHistory` needs, and the rule is the same rule.
//! - **A guessed exercise is left out.** Where nothing but a watch's classifier
//!   named the exercise, there is no claim about what was performed to compare
//!   against one.
//!
//! **So the shape is optional.** An exercise whose every set went, or an item
//! whose every exercise went, goes with them, and a session can be left with
//! nothing to read — which is a thing to say rather than an empty shape to
//! compare. No session in the operator's record reaches that state through
//! [`crate::prescription::project`]'s one caller, because a prescription exists
//! only for the years every set is logged in full; the type says so anyway.
//!
//! **Comparison is asymmetric, and that is a property of the domain rather than a
//! weakness of the test.** A performed six repetitions projects to `Exactly(6)`,
//! and the prescription may have said four to six. So [`satisfies`] treats a
//! projected `Exactly(n)` as agreeing with a prescribed `Range` containing `n`.
//! Equality on `WorkoutShape` is the wrong relation and is not the one used.
//!
//! **SC-010e is held by the compiler, and this is the test.** A projected shape
//! cannot be handed to anything that wants a prescription, because a
//! `PrescribedWorkout` carries the anchor, the week and the programme that only
//! generation has:
//!
//! ```compile_fail
//! use domain::{gym::CanonicalGymSession, prescription::{PrescribedWorkout, project}};
//!
//! // Standing in for `PrescribedWorkoutStore::issue`, which takes the same thing.
//! fn issue(_: &PrescribedWorkout) {}
//!
//! fn reverse_engineer_a_prescription(performed: &CanonicalGymSession) {
//!     let projection = project(performed);
//!     // The shape is instructional content and nothing more, so this does not
//!     // compile — which is FR-034 held by construction rather than by a rule
//!     // somebody has to follow.
//!     issue(projection.shape.as_ref().unwrap());
//! }
//! ```
//!
//! A runtime assertion here would be testing a rule that should not be
//! expressible in the first place.
//!
//! **And this one must compile**, which is what stops the test above from passing
//! for the wrong reason — a mistyped path or an unimported name would fail to
//! compile too, and would look exactly as green:
//!
//! ```
//! use domain::{
//!     gym::CanonicalGymSession,
//!     prescription::{PrescribedWorkout, WorkoutShape, project},
//! };
//!
//! fn issue(_: &PrescribedWorkout) {}
//! fn compare_against(_: &WorkoutShape) {}
//!
//! fn read_a_performance(performed: &CanonicalGymSession) {
//!     let projection = project(performed);
//!     // A shape is welcome wherever a shape is wanted. It is only `issue` that
//!     // refuses it, and refusing it is the whole point.
//!     if let Some(shape) = &projection.shape {
//!         compare_against(shape);
//!     }
//! }
//! ```
//!
//! **What this is not.** It is not correspondence. A projected shape is not the
//! prescription that motivated the performance and cannot be: a session can swap
//! an exercise, reorder items or abandon sets, and the projection describes the
//! result rather than recovering the intent. Comparing the two reports
//! divergences and asserts nothing about which one is right.

use std::collections::VecDeque;
use std::fmt;

use crate::canonical::Attributed;
use crate::gym::{
    CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Identified, Load,
    Performed, Rir, SetKind,
};
use crate::measure::Spans;
use crate::sequence::{AtLeastTwo, NonEmpty};

use super::{
    linear::{Position, PrimaryPattern},
    shape::{
        PrescribedExercise, PrescribedItem, PrescribedSuperset, SlotId, SupersetMember,
        WorkoutShape,
    },
    target::{Prescribed, PrescribedSet, Target},
};

/// Where in a workout's ordered sequence something sits.
///
/// **Zero-based, because the store is.** `workout_item.position` counts from
/// zero, so a gap reported here names the row it came from without arithmetic in
/// between.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemPosition(usize);

impl ItemPosition {
    pub const fn new(position: usize) -> Self {
        Self(position)
    }

    pub const fn as_usize(self) -> usize {
        self.0
    }
}

impl fmt::Display for ItemPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "item {}", self.0)
    }
}

/// What the performed record could not supply. Never filled with a guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionGap {
    /// A failed attempt carries the load and not the repetitions intended.
    IntendedMeasureUnknown { at: ItemPosition, load: Load },
    /// The template's issue order had no slot left for this item, so none was
    /// assigned and the item is not in the shape.
    SlotUnassignable { at: ItemPosition },
    /// A set the record does not state fully enough to read as an instruction:
    /// no load, no count where it completed, or nothing saying whether it was a
    /// warm-up. Left out of the shape, and its exercise with it where it was the
    /// only one.
    SetUnreadable { at: ItemPosition },
    /// Nothing but a watch's classifier named the exercise, so there is no
    /// account of what was performed to compare against a prescription.
    ExerciseGuessed { at: ItemPosition },
}

impl fmt::Display for ProjectionGap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IntendedMeasureUnknown { at, load } => write!(
                f,
                "{at}: a failed attempt at {load} does not record what was being attempted"
            ),
            Self::SlotUnassignable { at } => {
                write!(f, "{at}: no slot in the template's issue order")
            }
            Self::SetUnreadable { at } => write!(
                f,
                "{at}: the record does not state a set's load, count or kind"
            ),
            Self::ExerciseGuessed { at } => {
                write!(f, "{at}: only a watch's classifier named this exercise")
            }
        }
    }
}

/// A performance read as a prescription shape, and what was lost on the way.
///
/// **`None` where nothing survived the reading.** Every item's exercises were
/// guesses, or stated too little of their sets to be instructions, so there is
/// no shape to compare — which the gaps say in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    pub shape: Option<WorkoutShape>,
    pub gaps: Vec<ProjectionGap>,
}

/// The positions the template issues, against which items are read off.
///
/// [`PrimaryPattern::sequence`] itself, so the projection is walked against the
/// same sequence generation issues rather than a second copy of it.
///
/// **The primary is not distinguished here, and cannot be.** Generation issues
/// the primary before the upper pair, so a session whose primary is hip-dominant
/// issues its slots in a different order from this one — and which slot is
/// primary is authored programme data that a performed workout does not carry. So
/// positional assignment reads a hip-dominant primary as knee-dominant. That is
/// the sharpest edge of "slot identity is not in the performed record", it is
/// recorded rather than papered over, and it is why [`satisfies`] reports a slot
/// divergence rather than the projection refusing.
pub const ISSUE_ORDER: [Position; 10] = PrimaryPattern::KneeDominant.sequence();

/// Read a performed session as a prescription shape.
///
/// **Pure.** Reads no store, makes no request and consults no overlay, which is
/// why it is a function and not a port. It cannot fail either; what it can do is
/// come back with no shape, where nothing in the session was readable as an
/// instruction.
///
/// **The visit entire, seams included** (§ 3.1, § II.4). A visit the operator
/// logged across four Hevy routines, or recorded in a sheet and on a watch, is one
/// sequence of items by the time it reaches here. Where each field of it came from
/// is provenance, not shape, so nothing here can see the joins.
#[must_use]
pub fn project(session: &CanonicalGymSession) -> Projection {
    let mut positions: VecDeque<Position> = ISSUE_ORDER.into_iter().collect();
    let mut gaps = Vec::new();
    let mut projected = Vec::new();

    for (offset, item) in session.items().iter().enumerate() {
        let at = ItemPosition(offset);
        match assign(item, &mut positions, at, &mut gaps) {
            Some(item) => projected.push(item),
            None => gaps.push(ProjectionGap::SlotUnassignable { at }),
        }
    }

    Projection {
        shape: NonEmpty::new(projected).ok().map(WorkoutShape::new),
        gaps,
    }
}

/// Give one item the next position, consuming what it takes.
///
/// A single exercise against a supersetted position takes the first of the two
/// and hands the second back as a position of its own, which is what lets a pair
/// be read off when the operator performed it unsupersetted.
fn assign(
    item: &CanonicalItem,
    positions: &mut VecDeque<Position>,
    at: ItemPosition,
    gaps: &mut Vec<ProjectionGap>,
) -> Option<PrescribedItem> {
    let position = positions.pop_front()?;
    match (item, position) {
        (CanonicalItem::Exercise(_) | CanonicalItem::Superset(_), Position::Single(slot)) => {
            one_slot(item, slot, at, gaps)
        }
        (CanonicalItem::Exercise(_), Position::Superset(first, second)) => {
            positions.push_front(Position::Single(second));
            one_slot(item, first, at, gaps)
        }
        (CanonicalItem::Exercise(_), Position::Circuit(slots)) => {
            // The circuit was performed one exercise at a time. Its remaining
            // slots go back as positions of their own, in order.
            let (first, rest) = slots.split_first()?;
            for slot in rest.iter().rev() {
                positions.push_front(Position::Single(*slot));
            }
            one_slot(item, *first, at, gaps)
        }
        (CanonicalItem::Superset(members), Position::Circuit(slots)) => {
            if members.count() != slots.len() {
                // Putting the position back keeps the walk aligned for whatever
                // follows.
                positions.push_front(position);
                return None;
            }
            let tagged: Vec<SupersetMember> = members
                .iter()
                .zip(slots)
                .filter_map(|(performed, slot)| {
                    Some(SupersetMember {
                        slot,
                        exercise: exercise(performed, at, gaps)?,
                    })
                })
                .collect();
            grouped(tagged)
        }
        (CanonicalItem::Superset(members), Position::Superset(first, second)) => {
            let mut held = members.iter();
            let (Some(one), Some(two), None) = (held.next(), held.next(), held.next()) else {
                // More members than the pair has slots. Putting the position
                // back keeps the walk aligned for whatever follows.
                positions.push_front(position);
                return None;
            };
            let tagged: Vec<SupersetMember> = [(first, one), (second, two)]
                .into_iter()
                .filter_map(|(slot, performed)| {
                    Some(SupersetMember {
                        slot,
                        exercise: exercise(performed, at, gaps)?,
                    })
                })
                .collect();
            grouped(tagged)
        }
    }
}

/// The members that survived the reading, as the item they now make.
///
/// **One survivor is no longer a superset.** A pair of which only one exercise
/// could be read was performed back to back with something the record states too
/// little of, and calling what is left a superset would report a grouping the
/// shape does not hold. The gap already says what went.
fn grouped(members: Vec<SupersetMember>) -> Option<PrescribedItem> {
    AtLeastTwo::new(members)
        .ok()
        .map(|members| PrescribedItem::Superset(PrescribedSuperset { members }))
}

/// One item, every part of it tagged with the same slot.
///
/// A performed superset reaching a single position is work the template does not
/// pair — every member takes that one slot, and [`satisfies`] is left to report
/// the divergence.
fn one_slot(
    item: &CanonicalItem,
    slot: SlotId,
    at: ItemPosition,
    gaps: &mut Vec<ProjectionGap>,
) -> Option<PrescribedItem> {
    match item {
        CanonicalItem::Exercise(performed) => Some(PrescribedItem::Exercise {
            slot,
            exercise: exercise(performed, at, gaps)?,
        }),
        CanonicalItem::Superset(members) => {
            let tagged: Vec<SupersetMember> = members
                .iter()
                .filter_map(|performed| {
                    Some(SupersetMember {
                        slot,
                        exercise: exercise(performed, at, gaps)?,
                    })
                })
                .collect();
            grouped(tagged)
        }
    }
}

/// One canonical exercise as a prescribed one, measure partition intact.
///
/// `None` where only a watch's classifier named it, or where no set of it states
/// enough to be an instruction.
fn exercise(
    performed: &CanonicalExercise,
    at: ItemPosition,
    gaps: &mut Vec<ProjectionGap>,
) -> Option<PrescribedExercise> {
    match performed {
        CanonicalExercise::ForReps { identified, sets } => {
            let Identified::Recorded(exercise) = identified.copied() else {
                gaps.push(ProjectionGap::ExerciseGuessed { at });
                return None;
            };
            Some(PrescribedExercise::ForReps {
                exercise,
                sets: sets_of(sets, at, gaps)?,
            })
        }
        CanonicalExercise::ForDuration { exercise, sets } => {
            Some(PrescribedExercise::ForDuration {
                exercise: exercise.copied(),
                sets: sets_of(sets, at, gaps)?,
            })
        }
        CanonicalExercise::ForDistance { exercise, sets } => {
            Some(PrescribedExercise::ForDistance {
                exercise: exercise.copied(),
                sets: sets_of(sets, at, gaps)?,
            })
        }
    }
}

/// Every readable set of one exercise, order kept.
///
/// `None` where not one of them could be read, since an exercise with no sets is
/// not a prescription of anything.
fn sets_of<M: Copy + Spans>(
    sets: &NonEmpty<CanonicalSet<M>>,
    at: ItemPosition,
    gaps: &mut Vec<ProjectionGap>,
) -> Option<NonEmpty<PrescribedSet<M>>> {
    let read: Vec<PrescribedSet<M>> = sets
        .iter()
        .filter_map(|set| set_of(set, at, gaps))
        .collect();
    NonEmpty::new(read).ok()
}

/// One canonical set as a prescribed one.
///
/// A completed set pins both axes and carries no effort — the recorded RIR is an
/// observation and stays one. A failed attempt pins the load, leaves the measure
/// open and reports the gap, because what was being attempted is recorded
/// nowhere.
///
/// `None` where the record states no load, no count behind a completion, or
/// nothing about whether the set was a warm-up. Each of the three is a field a
/// canonical set may simply not hold, and an instruction cannot be read without
/// it.
fn set_of<M: Copy + Spans>(
    set: &CanonicalSet<M>,
    at: ItemPosition,
    gaps: &mut Vec<ProjectionGap>,
) -> Option<PrescribedSet<M>> {
    let unreadable = |gaps: &mut Vec<ProjectionGap>| {
        gaps.push(ProjectionGap::SetUnreadable { at });
        None
    };
    let (Some(load), Some(kind)) = (
        set.load.as_ref().map(Attributed::copied),
        set.kind.as_ref().map(Attributed::copied),
    ) else {
        return unreadable(gaps);
    };
    let prescription = match set.outcome.value() {
        Performed::Completed(Some(measure)) => Prescribed::Fixed {
            load,
            measure: Target::Exactly(*measure),
            effort: None,
        },
        Performed::Completed(None) => return unreadable(gaps),
        Performed::Failed => {
            gaps.push(ProjectionGap::IntendedMeasureUnknown { at, load });
            Prescribed::ToEffort {
                load,
                effort: Rir::Zero,
                predicted: None,
            }
        }
    };
    Some(PrescribedSet {
        prescription,
        rest_after: set
            .rest_after
            .as_ref()
            .map(Attributed::copied)
            .map(Target::Exactly),
        warmup: kind == SetKind::Warmup,
    })
}

/// One way a performance and a prescription part company.
///
/// **A report rather than a computation.** The locus is typed — position, member,
/// set — and the two values are rendered, because the three measures are three
/// types and a divergence that was generic over them could not be collected into
/// one list. What matters is that every divergence names itself; nothing consumes
/// these numerically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Divergence {
    /// A different number of items entirely.
    ItemCount { performed: usize, prescribed: usize },
    /// One side supersetted where the other did not.
    Grouping {
        at: ItemPosition,
        performed: bool,
        prescribed: bool,
    },
    Slot {
        at: ItemPosition,
        member: usize,
        performed: SlotId,
        prescribed: SlotId,
    },
    Exercise {
        at: ItemPosition,
        member: usize,
        performed: &'static str,
        prescribed: &'static str,
    },
    /// Counted in different things — repetitions against seconds.
    MeasureKind {
        at: ItemPosition,
        member: usize,
        performed: &'static str,
        prescribed: &'static str,
    },
    SetCount {
        at: ItemPosition,
        member: usize,
        performed: usize,
        prescribed: usize,
    },
    Load {
        at: ItemPosition,
        member: usize,
        set: usize,
        performed: String,
        prescribed: String,
    },
    /// The prescribed measure was not satisfied. Asymmetric: see
    /// [`Target::satisfied_by`].
    Measure {
        at: ItemPosition,
        member: usize,
        set: usize,
        performed: String,
        prescribed: String,
    },
}

impl fmt::Display for Divergence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ItemCount {
                performed,
                prescribed,
            } => write!(
                f,
                "{performed} items performed against {prescribed} prescribed"
            ),
            Self::Grouping {
                at,
                performed,
                prescribed,
            } => {
                let describe = |supersetted: bool| {
                    if supersetted {
                        "supersetted"
                    } else {
                        "on its own"
                    }
                };
                write!(
                    f,
                    "{at}: {} performed, {} prescribed",
                    describe(*performed),
                    describe(*prescribed)
                )
            }
            Self::Slot {
                at,
                member,
                performed,
                prescribed,
            } => write!(f, "{at} member {member}: {performed}, not {prescribed}"),
            Self::Exercise {
                at,
                member,
                performed,
                prescribed,
            } => write!(f, "{at} member {member}: {performed}, not {prescribed}"),
            Self::MeasureKind {
                at,
                member,
                performed,
                prescribed,
            } => write!(
                f,
                "{at} member {member}: counted in {performed}, prescribed in {prescribed}"
            ),
            Self::SetCount {
                at,
                member,
                performed,
                prescribed,
            } => write!(
                f,
                "{at} member {member}: {performed} sets against {prescribed} prescribed"
            ),
            Self::Load {
                at,
                member,
                set,
                performed,
                prescribed,
            } => write!(
                f,
                "{at} member {member} set {set}: {performed}, prescribed {prescribed}"
            ),
            Self::Measure {
                at,
                member,
                set,
                performed,
                prescribed,
            } => write!(
                f,
                "{at} member {member} set {set}: {performed} does not satisfy {prescribed}"
            ),
        }
    }
}

/// Does this performance's shape satisfy that prescription's?
///
/// **Empty means it does.** Every element of the answer is a way the two parted
/// company, so a session that did what it was told returns nothing.
///
/// **Asymmetric, and the direction is the point.** A prescribed range is
/// satisfied by a performed count inside it; the reverse is not a question worth
/// asking, because only one of the two is an instruction. Where the prescription
/// pins nothing — an open load, an open measure — there is nothing to satisfy and
/// nothing is reported.
#[must_use]
pub fn satisfies(performed: &WorkoutShape, prescribed: &WorkoutShape) -> Vec<Divergence> {
    let mut found = Vec::new();
    let (left, right) = (performed.items(), prescribed.items());
    if left.count() != right.count() {
        found.push(Divergence::ItemCount {
            performed: left.count(),
            prescribed: right.count(),
        });
    }

    let supersetted = |item: &PrescribedItem| matches!(item, PrescribedItem::Superset(_));
    for (offset, (performed, prescribed)) in left.iter().zip(right.iter()).enumerate() {
        let at = ItemPosition(offset);
        if supersetted(performed) != supersetted(prescribed) {
            found.push(Divergence::Grouping {
                at,
                performed: supersetted(performed),
                prescribed: supersetted(prescribed),
            });
        }
        compare_members(performed, prescribed, at, &mut found);
    }

    found
}

fn compare_members(
    performed: &PrescribedItem,
    prescribed: &PrescribedItem,
    at: ItemPosition,
    found: &mut Vec<Divergence>,
) {
    let left = performed.slots().zip(performed.exercises());
    let right = prescribed.slots().zip(prescribed.exercises());
    for (member, ((left_slot, left_exercise), (right_slot, right_exercise))) in
        left.zip(right).enumerate()
    {
        if left_slot != right_slot {
            found.push(Divergence::Slot {
                at,
                member,
                performed: left_slot,
                prescribed: right_slot,
            });
        }
        compare_exercises(left_exercise, right_exercise, at, member, found);
    }
}

fn compare_exercises(
    performed: &PrescribedExercise,
    prescribed: &PrescribedExercise,
    at: ItemPosition,
    member: usize,
    found: &mut Vec<Divergence>,
) {
    if performed.exercise_key() != prescribed.exercise_key() {
        found.push(Divergence::Exercise {
            at,
            member,
            performed: performed.exercise_key(),
            prescribed: prescribed.exercise_key(),
        });
    }

    match (performed, prescribed) {
        (
            PrescribedExercise::ForReps { sets: left, .. },
            PrescribedExercise::ForReps { sets: right, .. },
        ) => compare_sets(left, right, at, member, found),
        (
            PrescribedExercise::ForDuration { sets: left, .. },
            PrescribedExercise::ForDuration { sets: right, .. },
        ) => compare_sets(left, right, at, member, found),
        (
            PrescribedExercise::ForDistance { sets: left, .. },
            PrescribedExercise::ForDistance { sets: right, .. },
        ) => compare_sets(left, right, at, member, found),
        // Counted in different things, so no set-by-set comparison is meaningful.
        _ => found.push(Divergence::MeasureKind {
            at,
            member,
            performed: performed.measure(),
            prescribed: prescribed.measure(),
        }),
    }
}

fn compare_sets<M: fmt::Display + PartialEq + PartialOrd + Spans>(
    performed: &NonEmpty<PrescribedSet<M>>,
    prescribed: &NonEmpty<PrescribedSet<M>>,
    at: ItemPosition,
    member: usize,
    found: &mut Vec<Divergence>,
) {
    if performed.count() != prescribed.count() {
        found.push(Divergence::SetCount {
            at,
            member,
            performed: performed.count(),
            prescribed: prescribed.count(),
        });
    }

    for (set, (left, right)) in performed.iter().zip(prescribed.iter()).enumerate() {
        // Where the prescription left an axis open there is nothing to satisfy.
        if let Some(wanted) = right.prescription.load() {
            match left.prescription.load() {
                Some(lifted) if lifted == wanted => {}
                lifted => found.push(Divergence::Load {
                    at,
                    member,
                    set,
                    performed: lifted
                        .map_or_else(|| "no load recorded".to_owned(), |load| load.to_string()),
                    prescribed: wanted.to_string(),
                }),
            }
        }

        if let Some(wanted) = right.prescription.measure() {
            let satisfied = match left.prescription.measure() {
                Some(Target::Exactly(done)) => wanted.satisfied_by(done),
                // A projection never produces a range, so this is a prescription
                // compared against a prescription: nothing weaker than equality
                // is defensible there.
                Some(range) => range == wanted,
                None => false,
            };
            if !satisfied {
                found.push(Divergence::Measure {
                    at,
                    member,
                    set,
                    performed: left
                        .prescription
                        .measure()
                        .map_or_else(|| "an attempt with no count".to_owned(), Target::to_string),
                    prescribed: wanted.to_string(),
                });
            }
        }
    }
}
