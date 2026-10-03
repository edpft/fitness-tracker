//! A session performed against the template projects back into a prescription.
//!
//! The forward invariant of § 11, as a property rather than as a comparison
//! against history. A session performed on this platform is performed against a
//! generated prescription, so its structure is the template's: eleven items in
//! the order [`PrimaryPattern::sequence`] fixes, each one an exercise, a
//! supersetted pair, or the stretch circuit. What is generated here is any such
//! session — every exercise the vocabulary offers, every load, every outcome —
//! and the claim is that each one derives a prescription with no slot left
//! unassigned.
//!
//! **The session is a canonical one** (#350), which is what projection now
//! reads. Where this used to generate a session split across one to four Hevy
//! records, to show that projection cannot see the seams, the seams are now the
//! merge's business and `canonicalise.rs` is where they are tested: a canonical
//! session holds its items and nothing says which account each came from.
//!
//! **Every field a shape needs is stated here, and that is the point of the
//! separate cases below.** A canonical set may hold no load and no kind, and a
//! canonical exercise may be nothing but a watch's guess; a session shaped by the
//! template was logged in full, so the property generates full sets and the
//! unit tests cover what happens when the record holds less.
//!
//! **Nothing here reads the corpus.** The record predates the template and was
//! run by hand; comparing it against a regenerated prescription measures how the
//! operator's habits differed from the model, which is a fact about history
//! rather than a property of the model.

use domain::{
    canonical::{Attributed, NormalisedSessionId, Occurred},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Guess,
        GuessedExercise, Identified, Load, Performed, SetKind, SignedKg,
        exercise::{DurationExercise, RepsExercise},
    },
    measure::{Duration, Kg, RepCount},
    normalised::{OperatorZone, StartedAt},
    prescription::{Position, PrimaryPattern, ProjectionGap, SlotId, project},
    sequence::{AtLeastTwo, NonEmpty},
};
use proptest::prelude::*;

/// The one account every generated field is attributed to.
const ACCOUNT: NormalisedSessionId = NormalisedSessionId::FIRST;

// Strategy helpers are free functions, where the test exemptions in `clippy.toml`
// do not reach. Each is fallible and the caller filters.

fn load() -> impl Strategy<Value = Load> {
    prop_oneof![
        (0_u64..500_000).prop_map(|grams| Load::absolute(Kg::from_grams(grams))),
        (-100_000_i64..500_000).prop_map(|grams| Load::relative(SignedKg::from_grams(grams))),
    ]
}

/// One fully recorded set: a load, a kind, and an outcome.
fn set_of<M: std::fmt::Debug + Clone + 'static>(
    measure: impl Strategy<Value = M>,
) -> impl Strategy<Value = CanonicalSet<M>> {
    // Completed and failed both: a session that missed a set is still a session
    // performed against the template, and it must still project.
    (load(), measure, any::<bool>(), any::<bool>()).prop_map(
        |(load, measure, completed, warmup)| CanonicalSet {
            outcome: Attributed::new(
                if completed {
                    Performed::Completed(Some(measure))
                } else {
                    Performed::Failed
                },
                ACCOUNT,
            ),
            load: Some(Attributed::new(load, ACCOUNT)),
            began: None,
            intensity: None,
            kind: Some(Attributed::new(
                if warmup {
                    SetKind::Warmup
                } else {
                    SetKind::Working
                },
                ACCOUNT,
            )),
            rest_after: None,
        },
    )
}

fn non_empty<T: std::fmt::Debug + 'static>(
    element: impl Strategy<Value = T>,
) -> impl Strategy<Value = NonEmpty<T>> {
    prop::collection::vec(element, 1..5).prop_filter_map("one or more is a NonEmpty", |items| {
        NonEmpty::new(items).ok()
    })
}

/// One exercise, in whichever measure the slot's block is counted in.
///
/// The mobility block is held, so it draws from the duration vocabulary; every
/// other slot is counted in repetitions. That is the template's own partition,
/// and a session that ignored it would not be one performed against the
/// template.
fn exercise_for(slot: SlotId) -> BoxedStrategy<CanonicalExercise> {
    if slot.block() == domain::prescription::Block::Mobility {
        let seconds = (1_u64..3_600).prop_map(Duration::from_seconds);
        (
            0_usize..DurationExercise::ALL.len(),
            non_empty(set_of(seconds)),
        )
            .prop_filter_map("the vocabulary is not empty", |(at, sets)| {
                DurationExercise::ALL
                    .get(at)
                    .map(|&exercise| CanonicalExercise::ForDuration {
                        exercise: Attributed::new(exercise, ACCOUNT),
                        sets,
                    })
            })
            .boxed()
    } else {
        let reps =
            (1_u32..50).prop_filter_map("a non-zero rep count", |reps| RepCount::new(reps).ok());
        (0_usize..RepsExercise::ALL.len(), non_empty(set_of(reps)))
            .prop_filter_map("the vocabulary is not empty", |(at, sets)| {
                RepsExercise::ALL
                    .get(at)
                    .map(|&exercise| CanonicalExercise::ForReps {
                        identified: Attributed::new(Identified::Recorded(exercise), ACCOUNT),
                        sets,
                    })
            })
            .boxed()
    }
}

/// One item, shaped as the position it fills.
fn item_for(position: Position) -> BoxedStrategy<CanonicalItem> {
    if let Position::Single(slot) = position {
        return exercise_for(slot).prop_map(CanonicalItem::Exercise).boxed();
    }
    let members: Vec<BoxedStrategy<CanonicalExercise>> =
        position.slots().map(exercise_for).collect();
    members
        .prop_filter_map("a group has at least two members", |members| {
            AtLeastTwo::new(members)
                .ok()
                .map(|members| CanonicalItem::Superset(Box::new(members)))
        })
        .boxed()
}

/// A canonical session performed against one variant of the template.
fn performed(primary: PrimaryPattern) -> impl Strategy<Value = CanonicalGymSession> {
    let items: Vec<BoxedStrategy<CanonicalItem>> =
        primary.sequence().into_iter().map(item_for).collect();
    (items, 0_i64..1_000_000_000).prop_filter_map("a session is buildable", |(items, seconds)| {
        let zone = OperatorZone::try_from("Europe/London").ok()?;
        let instant = jiff::Timestamp::from_second(seconds).ok()?;
        Some(CanonicalGymSession::new(
            Occurred::At(StartedAt::new(instant, zone)),
            NonEmpty::new(items).ok()?,
            None,
            None,
        ))
    })
}

proptest! {
    /// Every session performed against the template derives a prescription.
    ///
    /// The projection walks positionally, so the claim is that walking the
    /// template's own sequence against a session shaped by it consumes exactly
    /// the positions there are: no item left without a slot, and no slot left
    /// unfilled.
    #[test]
    fn a_session_performed_against_the_template_projects_completely(
        session in performed(PrimaryPattern::KneeDominant)
    ) {
        let projection = project(&session);

        let unassignable = projection
            .gaps
            .iter()
            .filter(|gap| matches!(gap, ProjectionGap::SlotUnassignable { .. }))
            .count();
        prop_assert_eq!(unassignable, 0, "every item took a position");
        let shape = projection.shape.expect("a session logged in full has a shape");
        prop_assert_eq!(
            shape.items().count(),
            session.items().count(),
            "every item survives into the shape"
        );

        for slot in SlotId::ALL {
            prop_assert!(
                shape.item_for(*slot).is_some(),
                "the {} slot is filled",
                slot
            );
        }
        prop_assert!(shape.set_count() > 0);
    }
}

proptest! {
    /// The hip-dominant variant projects the same way, lower pair swapped.
    ///
    /// Separate rather than a strategy over both, so a failure names which
    /// variant broke without the reader decoding a shrunk enum.
    #[test]
    fn the_hip_dominant_variant_projects_the_same(
        session in performed(PrimaryPattern::HipDominant)
    ) {
        let projection = project(&session);
        let shape = projection.shape.expect("a session logged in full has a shape");
        prop_assert_eq!(shape.items().count(), session.items().count());
    }
}

/// A canonical session of one exercise, built from parts the caller states.
fn one_exercise(exercise: CanonicalExercise) -> Option<CanonicalGymSession> {
    let zone = OperatorZone::try_from("Europe/London").ok()?;
    Some(CanonicalGymSession::new(
        Occurred::At(StartedAt::new(jiff::Timestamp::from_second(0).ok()?, zone)),
        NonEmpty::of(CanonicalItem::Exercise(exercise), Vec::new()),
        None,
        None,
    ))
}

/// One set of repetitions, with whichever of its three fields the caller states.
fn bare_set(
    load: Option<Load>,
    kind: Option<SetKind>,
    reps: Option<RepCount>,
) -> CanonicalSet<RepCount> {
    CanonicalSet {
        outcome: Attributed::new(Performed::Completed(reps), ACCOUNT),
        load: load.map(|load| Attributed::new(load, ACCOUNT)),
        began: None,
        intensity: None,
        kind: kind.map(|kind| Attributed::new(kind, ACCOUNT)),
        rest_after: None,
    }
}

/// A set the record states no load for cannot be an instruction, and a session
/// of nothing but such sets has no shape at all.
#[test]
fn a_set_with_no_load_is_not_read_as_a_prescription() {
    let reps = RepCount::new(5).expect("a real count");
    let exercise = CanonicalExercise::ForReps {
        identified: Attributed::new(
            Identified::Recorded(RepsExercise::BackSquatBarbell),
            ACCOUNT,
        ),
        sets: NonEmpty::of(
            bare_set(None, Some(SetKind::Working), Some(reps)),
            Vec::new(),
        ),
    };
    let session = one_exercise(exercise).expect("a session builds");
    let projection = project(&session);
    assert!(projection.shape.is_none(), "nothing was readable");
    assert!(
        projection
            .gaps
            .iter()
            .any(|gap| matches!(gap, ProjectionGap::SetUnreadable { .. })),
        "the gap says which set went: {:?}",
        projection.gaps
    );
}

/// The same for a set nothing says was a warm-up or a working set: reading it as
/// working would compare a ramp against a prescribed working set.
#[test]
fn a_set_with_no_kind_is_not_read_as_a_prescription() {
    let reps = RepCount::new(5).expect("a real count");
    let load = Load::absolute(Kg::from_grams(60_000));
    let exercise = CanonicalExercise::ForReps {
        identified: Attributed::new(
            Identified::Recorded(RepsExercise::BackSquatBarbell),
            ACCOUNT,
        ),
        sets: NonEmpty::of(bare_set(Some(load), None, Some(reps)), Vec::new()),
    };
    let session = one_exercise(exercise).expect("a session builds");
    assert!(project(&session).shape.is_none());
}

/// A set the gym's log recorded without a count — "Burpee", and not how many —
/// pins no measure, so it is not an instruction either.
#[test]
fn a_completed_set_with_no_count_is_not_read_as_a_prescription() {
    let load = Load::absolute(Kg::from_grams(60_000));
    let exercise = CanonicalExercise::ForReps {
        identified: Attributed::new(
            Identified::Recorded(RepsExercise::BackSquatBarbell),
            ACCOUNT,
        ),
        sets: NonEmpty::of(
            bare_set(Some(load), Some(SetKind::Working), None),
            Vec::new(),
        ),
    };
    let session = one_exercise(exercise).expect("a session builds");
    assert!(project(&session).shape.is_none());
}

/// A watch's guess is not an account of what was performed, so nothing is
/// compared against it however completely its sets are stated.
#[test]
fn a_guessed_exercise_is_not_compared_against_a_prescription() {
    let reps = RepCount::new(5).expect("a real count");
    let load = Load::absolute(Kg::from_grams(60_000));
    let exercise = CanonicalExercise::ForReps {
        identified: Attributed::new(
            Identified::Guessed(GuessedExercise::Proposed(Guess::Exercise(
                RepsExercise::BackSquatBarbell,
            ))),
            ACCOUNT,
        ),
        sets: NonEmpty::of(
            bare_set(Some(load), Some(SetKind::Working), Some(reps)),
            Vec::new(),
        ),
    };
    let session = one_exercise(exercise).expect("a session builds");
    let projection = project(&session);
    assert!(projection.shape.is_none());
    assert!(
        projection
            .gaps
            .iter()
            .any(|gap| matches!(gap, ProjectionGap::ExerciseGuessed { .. })),
        "the gap names the guess: {:?}",
        projection.gaps
    );
}
