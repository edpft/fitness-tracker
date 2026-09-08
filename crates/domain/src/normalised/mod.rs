//! What the normalised layer needs whatever entity it is deriving.
//!
//! **Not the gym's, and it used to be.** Every type here lived in
//! [`crate::gym`] while there was one normalised entity, which read as though a
//! refusal, a derivation run and a declared time zone were facts about lifting.
//! They are facts about § II.3 — the layer that says what each source said, in
//! our terms — and the second entity is what made the difference visible: a
//! Peloton ride refuses records, is written by a run and starts at a zoned
//! instant, and none of that should be reached for through the gym.
//!
//! What is *not* here is the entities themselves. A [`crate::gym::GymWorkout`]
//! and a [`crate::cycling::BikePlusRide`] are declared by the discipline they
//! belong to; this is the vocabulary they are derived with.

pub mod entity;
pub mod refusal;
pub mod run;
pub mod time;

pub use entity::NormalisedEntity;
pub use refusal::{Refusal, RefusalKind, RefusalLocus, RefusalReason};
pub use run::{
    NormalisationFailure, NormalisationOutcome, NormalisationRun, NormalisationRunId, RefusalCount,
    UnknownNormalisationFailure, WorkoutCount,
};
pub use time::{OperatorZone, StartedAt, UnknownTimeZone};
