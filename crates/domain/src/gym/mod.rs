//! The gym workout: the normalised layer's entity for what happens in a gym.
//!
//! Declared, version-controlled and owned here (§ II.3), and sources are
//! translated into it rather than the reverse. Nothing in this module knows
//! that Hevy exists — the vocabulary is ours, and the mapping from any one
//! source's identifiers onto it lives with that source's adapter.
//!
//! The reasoning behind every type is in `docs/gym-workout-domain-model.md`,
//! which is the model of record. What is restated here is only what a reader of
//! the code needs in order not to undo it.

pub mod exercise;
pub mod intensity;
pub mod load;
pub mod measure;
pub mod outcome;
pub mod refusal;
pub mod run;
pub mod sequence;
pub mod set;
pub mod time;
pub mod workout;

pub use exercise::{DistanceExercise, DurationExercise, Exercise, RepsExercise, UnknownExercise};
pub use intensity::{Rir, UnrecognisedIntensity};
pub use load::{InvalidLoad, Kg, Load, SignedKg};
pub use measure::{
    Distance, Duration, InvalidQuantity, Metres, PositiveDistance, PositiveDuration, RepCount,
    Spans,
};
pub use outcome::Performed;
pub use refusal::{Refusal, RefusalKind, RefusalLocus, RefusalReason};
pub use run::{
    NormalisationFailure, NormalisationOutcome, NormalisationRun, NormalisationRunId, RefusalCount,
    UnknownNormalisationFailure, WorkoutCount,
};
pub use sequence::{AtLeastTwo, NonEmpty, TooShort};
pub use set::{Set, SetKind};
pub use time::{OperatorZone, UnknownTimeZone, WorkoutStart};
pub use workout::{GymWorkout, PerformedExercise, Superset, WorkoutItem};
