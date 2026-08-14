//! The gym workout: ordered items, each an exercise or a superset.
//!
//! It is a gym workout, not a strength workout. 208 sets in the corpus are
//! running, skipping, sled work, stretching and isometric holds, and what the
//! entity is does not depend on which device recorded it — a session recorded
//! on a watch and the same session recorded here are two observations of one
//! event, reconciled at the canonical layer.
//!
//! There is deliberately no session above this. The source's workout boundary is
//! not the session boundary — two days in the corpus landed four back-to-back
//! records each, one training session fragmented by an attempt to make parts
//! reusable — but a session stands for more than one normalised entity, which
//! makes it composition at the canonical layer rather than anything this layer
//! can build.

use std::fmt;

use crate::landing::{LandingRecordId, Provenance, SourceRecordId};

use super::{
    exercise::{DistanceExercise, DurationExercise, RepsExercise, TimedDistanceExercise},
    measure::{Distance, Duration, RepCount, TimedDistance},
    nonempty::{AtLeastTwo, NonEmpty},
    set::Set,
    time::WorkoutStart,
};

/// One exercise together with the sets performed of it.
///
/// "Exercise" is the right word for the container even though the literature
/// spends it on the movement. Programming compresses — `back squat 3×5 @ 80%` —
/// in a way recording cannot, since each performed set has its own reps,
/// intensity and rest, so the literature has no term for this and inventing one
/// is legitimate. An exercise holding its sets will not mislead anyone.
///
/// Which variant this is fixes the measure, so a set and its exercise cannot
/// disagree and nothing validates the pairing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerformedExercise {
    ForReps {
        exercise: RepsExercise,
        sets: NonEmpty<Set<RepCount>>,
    },
    ForDuration {
        exercise: DurationExercise,
        sets: NonEmpty<Set<Duration>>,
    },
    ForDistance {
        exercise: DistanceExercise,
        sets: NonEmpty<Set<Distance>>,
    },
    ForTimedDistance {
        exercise: TimedDistanceExercise,
        sets: NonEmpty<Set<TimedDistance>>,
    },
}

impl PerformedExercise {
    /// The exercise's stable key, whichever vocabulary it came from.
    pub const fn exercise_key(&self) -> &'static str {
        match self {
            Self::ForReps { exercise, .. } => exercise.as_str(),
            Self::ForDuration { exercise, .. } => exercise.as_str(),
            Self::ForDistance { exercise, .. } => exercise.as_str(),
            Self::ForTimedDistance { exercise, .. } => exercise.as_str(),
        }
    }

    pub const fn measure(&self) -> &'static str {
        match self {
            Self::ForReps { .. } => "reps",
            Self::ForDuration { .. } => "duration",
            Self::ForDistance { .. } => "distance",
            Self::ForTimedDistance { .. } => "timed-distance",
        }
    }

    /// How many sets were performed. Never zero.
    pub fn set_count(&self) -> usize {
        match self {
            Self::ForReps { sets, .. } => sets.count(),
            Self::ForDuration { sets, .. } => sets.count(),
            Self::ForDistance { sets, .. } => sets.count(),
            Self::ForTimedDistance { sets, .. } => sets.count(),
        }
    }
}

impl fmt::Display for PerformedExercise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} × {}", self.exercise_key(), self.set_count())
    }
}

/// Exercises performed back to back.
///
/// That is the definition, so its members are contiguous and there are at least
/// two of them, and a container in the ordered sequence is the type that says
/// so. Contiguity is not carried here — a superset holds its members and knows
/// nothing of the sequence it came from — because it is a property of the
/// record's ordering, checked once where that ordering is visible. Nothing
/// downstream can break it, since nothing downstream reorders a workout.
///
/// Grouping *sets* rather than exercises is arguably more faithful to what
/// happens, and was rejected: it would require exercise identity to move onto
/// the set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Superset {
    pub members: AtLeastTwo<PerformedExercise>,
}

/// One position in a workout's ordered sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkoutItem {
    Exercise(PerformedExercise),
    Superset(Superset),
}

impl WorkoutItem {
    /// Every performed exercise in this item, in order — one, or a superset's
    /// members.
    pub fn exercises(&self) -> Box<dyn Iterator<Item = &PerformedExercise> + '_> {
        match self {
            Self::Exercise(exercise) => Box::new(std::iter::once(exercise)),
            Self::Superset(superset) => Box::new(superset.members.iter()),
        }
    }

    pub const fn is_superset(&self) -> bool {
        matches!(self, Self::Superset(_))
    }
}

/// One workout, as one source recorded it.
///
/// Identified by the landing record it came from, not by the source's record
/// id. Two records sharing a source id are the same source contradicting
/// itself, and § 10 puts that at the canonical layer — so both produce a
/// workout here and both stand. Keying on the source id would collapse the pair
/// silently, which is the one thing this layer must not do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GymWorkout {
    items: NonEmpty<WorkoutItem>,
    started_at: WorkoutStart,
    provenance: Provenance,
    source_record_id: SourceRecordId,
    landed_as: LandingRecordId,
}

impl GymWorkout {
    /// Provenance is a constructor argument rather than a setter, so a workout
    /// that exists is a workout that knows where it came from (§ II.3).
    pub const fn new(
        items: NonEmpty<WorkoutItem>,
        started_at: WorkoutStart,
        provenance: Provenance,
        source_record_id: SourceRecordId,
        landed_as: LandingRecordId,
    ) -> Self {
        Self {
            items,
            started_at,
            provenance,
            source_record_id,
            landed_as,
        }
    }

    pub const fn items(&self) -> &NonEmpty<WorkoutItem> {
        &self.items
    }

    pub const fn started_at(&self) -> &WorkoutStart {
        &self.started_at
    }

    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    pub const fn source_record_id(&self) -> &SourceRecordId {
        &self.source_record_id
    }

    pub const fn landed_as(&self) -> LandingRecordId {
        self.landed_as
    }

    /// Every performed exercise, flattened across items in their recorded
    /// order. What an entry count is over.
    pub fn exercises(&self) -> impl Iterator<Item = &PerformedExercise> {
        self.items.iter().flat_map(WorkoutItem::exercises)
    }

    pub fn set_count(&self) -> usize {
        self.exercises()
            .map(PerformedExercise::set_count)
            .sum::<usize>()
    }

    pub fn superset_count(&self) -> usize {
        self.items.iter().filter(|item| item.is_superset()).count()
    }
}

impl fmt::Display for GymWorkout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {} items, {} sets",
            self.started_at,
            self.items.count(),
            self.set_count()
        )
    }
}
