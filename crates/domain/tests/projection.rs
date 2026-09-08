//! A workout performed against the template projects back into a prescription.
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
//! **Nothing here reads the corpus.** The record predates the template and was
//! run by hand; comparing it against a regenerated prescription measures how the
//! operator's habits differed from the model, which is a fact about history
//! rather than a property of the model.

use domain::{
    gym::{
        GymWorkout, Kg, Load, Performed, PerformedExercise, PerformedGymSession, Set, SetKind,
        SignedKg, Superset, WorkoutItem,
        exercise::{DurationExercise, RepsExercise},
    },
    landing::{Endpoint, EventKind, EventProvenance, LandingRecordId, Provenance, SourceRecordId},
    measure::{Duration, RepCount},
    normalised::{OperatorZone, StartedAt},
    prescription::{Position, PrimaryPattern, ProjectionGap, SlotId, project},
    sequence::{AtLeastTwo, NonEmpty},
};
use proptest::prelude::*;

// Strategy helpers are free functions, where the test exemptions in `clippy.toml`
// do not reach. Each is fallible and the caller filters.

fn load() -> impl Strategy<Value = Load> {
    prop_oneof![
        (0_u64..500_000).prop_map(|grams| Load::absolute(Kg::from_grams(grams))),
        (-100_000_i64..500_000).prop_map(|grams| Load::relative(SignedKg::from_grams(grams))),
    ]
}

fn set_of<M: std::fmt::Debug + Clone + 'static>(
    measure: impl Strategy<Value = M>,
) -> impl Strategy<Value = Set<M>> {
    // Completed and failed both: a session that missed a set is still a session
    // performed against the template, and it must still project.
    (load(), measure, any::<bool>(), any::<bool>()).prop_map(
        |(load, measure, completed, warmup)| Set {
            load,
            outcome: if completed {
                Performed::Completed(measure)
            } else {
                Performed::Failed
            },
            intensity: None,
            kind: if warmup {
                SetKind::Warmup
            } else {
                SetKind::Working
            },
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
fn exercise_for(slot: SlotId) -> BoxedStrategy<PerformedExercise> {
    if slot.block() == domain::prescription::Block::Mobility {
        let seconds = (1_u64..3_600).prop_map(Duration::from_seconds);
        (
            0_usize..DurationExercise::ALL.len(),
            non_empty(set_of(seconds)),
        )
            .prop_filter_map("the vocabulary is not empty", |(at, sets)| {
                DurationExercise::ALL
                    .get(at)
                    .map(|&exercise| PerformedExercise::ForDuration { exercise, sets })
            })
            .boxed()
    } else {
        let reps =
            (1_u32..50).prop_filter_map("a non-zero rep count", |reps| RepCount::new(reps).ok());
        (0_usize..RepsExercise::ALL.len(), non_empty(set_of(reps)))
            .prop_filter_map("the vocabulary is not empty", |(at, sets)| {
                RepsExercise::ALL
                    .get(at)
                    .map(|&exercise| PerformedExercise::ForReps { exercise, sets })
            })
            .boxed()
    }
}

/// One item, shaped as the position it fills.
fn item_for(position: Position) -> BoxedStrategy<WorkoutItem> {
    if let Position::Single(slot) = position {
        return exercise_for(slot).prop_map(WorkoutItem::Exercise).boxed();
    }
    let members: Vec<BoxedStrategy<PerformedExercise>> =
        position.slots().map(exercise_for).collect();
    members
        .prop_filter_map("a group has at least two members", |members| {
            AtLeastTwo::new(members)
                .ok()
                .map(|members| WorkoutItem::Superset(Superset { members }))
        })
        .boxed()
}

/// A session performed against one variant of the template, split across one to
/// four records.
///
/// **The split is part of the property.** The operator logged single sessions
/// against several Hevy routines — 21 of his 140 days, three of them four
/// records — and § 3.1 makes those one entity. Projection reads the session's
/// items in the order performed and cannot see where the seams were, so a
/// session that landed as four records must project exactly as the same session
/// landed as one. Generating the split is what tests that rather than assuming
/// it.
fn performed(primary: PrimaryPattern) -> impl Strategy<Value = PerformedGymSession> {
    let items: Vec<BoxedStrategy<WorkoutItem>> =
        primary.sequence().into_iter().map(item_for).collect();
    (items, 0_i64..1_000_000_000, 1_usize..=4).prop_filter_map(
        "a session is buildable",
        |(items, seconds, parts)| {
            let zone = OperatorZone::try_from("Europe/London").ok()?;
            let provenance = Provenance::Event(EventProvenance::new(
                Endpoint::try_from("/v1/workouts/events").ok()?,
                EventKind::Updated,
                None,
            ));

            // Ceiling division, so every chunk holds at least one item and the
            // last is never empty — a workout's items are `NonEmpty`.
            let per_part = items.len().div_ceil(parts).max(1);
            let mut workouts = Vec::new();
            for (index, chunk) in items.chunks(per_part).enumerate() {
                let index = i64::try_from(index).ok()?;
                // Each part starts after the one before it, which is the order
                // grouping would have put them in.
                let instant = jiff::Timestamp::from_second(seconds + index * 600).ok()?;
                workouts.push(GymWorkout::new(
                    NonEmpty::new(chunk.to_vec()).ok()?,
                    StartedAt::new(instant, zone.clone()),
                    provenance.clone(),
                    SourceRecordId::try_from(format!("synthetic-{index}").as_str()).ok()?,
                    LandingRecordId::FIRST,
                    // Performed freehand: no session was delivered for it.
                    None,
                ));
            }
            Some(PerformedGymSession::new(NonEmpty::new(workouts).ok()?))
        },
    )
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
        prop_assert_eq!(
            projection.shape.items().count(),
            session.items().count(),
            "every item survives into the shape"
        );

        for slot in SlotId::ALL {
            prop_assert!(
                projection.shape.item_for(*slot).is_some(),
                "the {} slot is filled",
                slot
            );
        }
        prop_assert!(projection.shape.set_count() > 0);
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
        prop_assert_eq!(
            projection.shape.items().count(),
            session.items().count()
        );
    }
}
