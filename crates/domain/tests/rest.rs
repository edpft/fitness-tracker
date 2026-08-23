//! What a session instructs between its sets.
//!
//! The three states matter and are different: no instruction (the warm-up ramp),
//! an instruction to go straight on (a superset), and a length. These pin the
//! rule that tells them apart.

use domain::{
    gym::{
        Duration, Kg, Load, NonEmpty, RepCount,
        exercise::{DurationExercise, RepsExercise},
        sequence::AtLeastTwo,
    },
    prescription::{
        BlockRest, PrescribedExercise, PrescribedItem, PrescribedSet, PrescribedSuperset,
        RestScheme, SlotId, SupersetMember, Target, WorkoutShape, rested,
    },
};

type Built<T> = Result<T, Box<dyn std::error::Error>>;

const fn flat(seconds: u64) -> BlockRest {
    BlockRest {
        between_sets: Target::Exactly(Duration::from_seconds(seconds)),
        after_superset: None,
    }
}

fn grouped(low: u64, high: u64, ss_low: u64, ss_high: u64) -> Built<BlockRest> {
    Ok(BlockRest {
        between_sets: span(low, high)?,
        after_superset: Some(span(ss_low, ss_high)?),
    })
}

/// The operator's own, stated on 2026-08-23.
fn scheme() -> Built<RestScheme> {
    Ok(RestScheme {
        plyometric: flat(30),
        power: flat(90),
        strength: grouped(120, 180, 90, 150)?,
        hypertrophy: grouped(120, 180, 90, 150)?,
        mobility: flat(0),
    })
}

fn working(load: u64, reps: u32) -> Built<PrescribedSet<RepCount>> {
    Ok(PrescribedSet::fixed(
        Load::Absolute(Kg::from_grams(load)),
        Target::Exactly(RepCount::new(reps)?),
    ))
}

fn warmup(load: u64, reps: u32) -> Built<PrescribedSet<RepCount>> {
    Ok(PrescribedSet::warmup(
        Load::Absolute(Kg::from_grams(load)),
        Target::Exactly(RepCount::new(reps)?),
    ))
}

/// The rest each set of an exercise instructs, in order.
type Rests = Vec<Option<Target<Duration>>>;

fn rests_of(exercise: &PrescribedExercise) -> Rests {
    match exercise {
        PrescribedExercise::ForReps { sets, .. } => sets.iter().map(|set| set.rest_after).collect(),
        PrescribedExercise::ForDuration { sets, .. } => {
            sets.iter().map(|set| set.rest_after).collect()
        }
        PrescribedExercise::ForDistance { sets, .. } => {
            sets.iter().map(|set| set.rest_after).collect()
        }
    }
}

fn first(shape: &WorkoutShape) -> Rests {
    match shape.items().first() {
        PrescribedItem::Exercise { exercise, .. } => rests_of(exercise),
        PrescribedItem::Superset(superset) => rests_of(&superset.members.first().exercise),
    }
}

/// The primary's ramp and its working sets.
fn ramp() -> Built<WorkoutShape> {
    let squat = PrescribedExercise::ForReps {
        exercise: RepsExercise::FrontSquat,
        sets: NonEmpty::new(vec![
            warmup(30_000, 4)?,
            warmup(60_000, 2)?,
            working(80_000, 3)?,
            working(70_000, 6)?,
        ])?,
    };
    Ok(WorkoutShape::new(NonEmpty::new(vec![
        PrescribedItem::Exercise {
            slot: SlotId::KneeDominant,
            exercise: squat,
        },
    ])?))
}

/// A pair of arms slots, supersetted.
fn arms() -> Built<WorkoutShape> {
    let member = |slot, exercise| -> Built<SupersetMember> {
        Ok(SupersetMember {
            slot,
            exercise: PrescribedExercise::ForReps {
                exercise,
                sets: NonEmpty::new(vec![working(20_000, 6)?])?,
            },
        })
    };

    Ok(WorkoutShape::new(NonEmpty::new(vec![
        PrescribedItem::Superset(PrescribedSuperset {
            members: AtLeastTwo::new(vec![
                member(SlotId::Biceps, RepsExercise::PreacherCurlBarbell)?,
                member(SlotId::Triceps, RepsExercise::TricepsExtensionCable)?,
            ])?,
        }),
    ])?))
}

/// The two holds, supersetted.
fn holds() -> Built<WorkoutShape> {
    let hold = |exercise| -> Built<PrescribedExercise> {
        Ok(PrescribedExercise::ForDuration {
            exercise,
            sets: NonEmpty::new(vec![PrescribedSet::fixed(
                Load::UNLOADED,
                Target::Exactly(Duration::from_seconds(60)),
            )])?,
        })
    };

    Ok(WorkoutShape::new(NonEmpty::new(vec![
        PrescribedItem::Superset(PrescribedSuperset {
            members: AtLeastTwo::new(vec![
                SupersetMember {
                    slot: SlotId::HandstandHold,
                    exercise: hold(DurationExercise::HandstandHold)?,
                },
                SupersetMember {
                    slot: SlotId::DeadHang,
                    exercise: hold(DurationExercise::DeadHang)?,
                },
            ])?,
        }),
    ])?))
}

/// Two plyometric sets.
fn pogos() -> Built<WorkoutShape> {
    let pogo = PrescribedExercise::ForReps {
        exercise: RepsExercise::Pogo,
        sets: NonEmpty::new(vec![working(0, 20)?, working(0, 20)?])?,
    };
    Ok(WorkoutShape::new(NonEmpty::new(vec![
        PrescribedItem::Exercise {
            slot: SlotId::Plyometric,
            exercise: pogo,
        },
    ])?))
}

/// The two members of the one item in a shape.
fn pair(shape: &WorkoutShape) -> Built<(Rests, Rests)> {
    let PrescribedItem::Superset(superset) = shape.items().first() else {
        return Err("the item is a superset".into());
    };
    Ok((
        rests_of(&superset.members.first().exercise),
        rests_of(&superset.members.second().exercise),
    ))
}

fn span(low: u64, high: u64) -> Built<Target<Duration>> {
    Target::between(Duration::from_seconds(low), Duration::from_seconds(high))
        .ok_or_else(|| "a rest range must span".into())
}

/// **A warm-up instructs nothing, and its last step instructs the least the
/// block asks for.** Changing the plates is the rest, and nothing prescribes how
/// long that takes — but the step into the working set is the first time the ramp
/// asks for something.
#[test]
fn a_warm_up_ramp_rests_only_on_its_way_into_the_working_set() {
    let shape = ramp().expect("the ramp fixture builds");
    let scheme = scheme().expect("the rest scheme builds");
    let range = span(120, 180).expect("two to three minutes spans");
    let two_minutes = Target::Exactly(Duration::from_seconds(120));

    assert_eq!(
        first(&rested(&shape, &scheme)),
        vec![None, Some(two_minutes), Some(range), Some(range)],
        "no rest between warm-ups, the low end into the single, then the range"
    );
}

/// **Zero is an instruction and absent is not.** A superset tells you to go
/// straight into the next exercise; only the member the group ends on carries
/// the block's rest.
#[test]
fn a_superset_rests_only_where_it_ends() {
    let shape = arms().expect("the arms fixture builds");
    let scheme = scheme().expect("the rest scheme builds");
    let ending = span(90, 150).expect("ninety to a hundred and fifty spans");

    let (first_member, second_member) =
        pair(&rested(&shape, &scheme)).expect("the item is a superset");

    assert_eq!(
        first_member,
        vec![Some(Target::Exactly(Duration::ZERO))],
        "the first member runs straight into the second"
    );
    assert_eq!(
        second_member,
        vec![Some(ending)],
        "and the group rests once it ends, at the supersetted length"
    );
}

/// A block that states no superset rest rests the same however it is grouped.
/// Mobility rests not at all, which is zero rather than absent.
#[test]
fn a_block_with_no_superset_rest_rests_the_same_either_way() {
    let shape = holds().expect("the holds fixture builds");
    let scheme = scheme().expect("the rest scheme builds");

    let (first_member, second_member) =
        pair(&rested(&shape, &scheme)).expect("the item is a superset");

    let zero = vec![Some(Target::Exactly(Duration::ZERO))];
    assert_eq!(first_member, zero);
    assert_eq!(
        second_member, zero,
        "mobility rests for nothing, which is an instruction rather than a gap"
    );
}

/// A block stating one number uses it wherever it appears.
#[test]
fn a_block_stating_one_number_rests_that_long() {
    let shape = pogos().expect("the plyometric fixture builds");
    let scheme = scheme().expect("the rest scheme builds");

    let thirty = Some(Target::Exactly(Duration::from_seconds(30)));
    assert_eq!(first(&rested(&shape, &scheme)), vec![thirty, thirty]);
}
