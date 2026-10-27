//! The prescribed side of § 11: what we intend, as against what happened.
//!
//! Authored rather than observed, so nothing here acquires a raw, normalised or
//! canonical layer — § III governs it and § II does not reach it. The one
//! direction that is permitted runs inward: a prescription may be derived by
//! reading the performed record, and never the reverse.
//!
//! The reasoning behind every type is in
//! `docs/prescribed-workout-domain-model.md` and, for the primary strength slot
//! alone, `docs/primary-lift-progression.md`. Those are the models of record.
//! What is restated here is only what a reader of the code needs in order not to
//! undo it.
//!
//! **A plan and a mechanism for handling failure are two different things.**
//! [`ladder`] holds the plan — a percentage of a fixed anchor per climbing week,
//! generated from a duration and a starting 1RM. The failure mechanism lives
//! beside it and takes over when the plan turns out to have been too ambitious.
//! Neither is derived from the other.

pub mod anchor;
pub mod attempts;
pub mod authored;
pub mod block;
pub mod candidates;
pub mod change;
pub mod delivery;
pub mod design;
pub mod ladder;
pub mod linear;
pub mod mesocycle;
pub mod parameters;
pub mod prilepin;
pub mod progression;
pub mod project;
pub mod repmax;
pub mod rest;
pub mod sbs;
pub mod schedule;
pub mod seed;
pub mod shape;
pub mod steps;
pub mod succession;
pub mod target;
pub mod test;
pub mod warmup;
pub mod workout;

// `authored::Shape` is deliberately not re-exported here: `shape` is a module
// at this level and `WorkoutShape` is the shape a session has, so a bare `Shape`
// at the crate root would read as one of those. Reach it through
// `prescription::authored::Shape`, beside the `Authored` it belongs to.
pub use anchor::{Anchor, AnchorProvenance, InvalidAnchor, UnknownProvenance, in_force};
pub use attempts::Attempts;
pub use authored::{Authored, AuthoringError};
// `block::Block` is reached through `prescription::block::Block`, which is what
// its own module doc calls it. A group of items in one session is a `Section`
// (operator, 2026-10-26), which is what freed the word for periodisation.
pub use block::{BlockPeriodisation, BlockWeek, EntryTest, InvalidBlock, Phase, WeekPlan};
pub use change::{FillChange, FillChanges};
pub use delivery::{
    DeliveryReference, DestinationName, DestinationReply, InvalidDelivery, PrescriptionState,
    RenderingDigest, ReplyStatus, SessionOrdinal, WrongRenderingWidth,
};
pub use design::{AccessoryScheme, InvalidSuperset, Levels, Rest, RestScheme, Superset, Workout};
pub use ladder::{InvalidLadder, Ladder, Opening};
pub use linear::{
    BackOff, Fill, Linear, LinearProgression, LinearRest, Position, Primary, PrimaryPattern,
    ResetProtocol, SlotContent, SlotFills, StaticFill, TopSetReps,
};
pub use mesocycle::{GymMesocycle, InconsistentMesocycle, Progression, check_primary};
pub use parameters::{InvalidPercentage, Percentage, Scales};
pub use progression::{GatingTopSet, Progress, Reset, progress_after};
pub use project::{Divergence, ItemPosition, Projection, ProjectionGap, project, satisfies};
pub use repmax::rep_max;
pub use rest::{rested, warmup_rested};
pub use sbs::{
    CompletedSet, InvalidSbs, Sbs, SbsChart, SbsDay, SbsProgression, SbsRest, SbsSession, SbsWeek,
    achieved, advance, maximum_after, training_max_share, working_load,
};
// `SessionRole` is deliberately not re-exported here. It moved to
// `domain::schedule` on 2026-09-20 (issue #63), because a training slot carries
// one and both disciplines read it — a role is a fact about the operator's week
// rather than about the gym's prescriptions, and a second name for it at this
// level would invite the gym to keep thinking of it as its own.
pub use schedule::{
    ByIntensity, Calendar, Interruptions, InvalidCalendar, InvalidWeek, NotScheduled, Skip,
    WeekIndex, WeekKind,
};
pub use shape::{
    PrescribedExercise, PrescribedItem, PrescribedSuperset, Section, SlotId, SupersetMember,
    UnknownSection, UnknownSlot, WorkoutShape,
};
pub use steps::{InvalidLoadSteps, LoadSteps, Step};
pub use succession::{RECENT_WEEKS, is_recent_enough, weeks_between};
pub use target::{Prescribed, PrescribedSet, Target};
pub use test::{Test, Tested};
pub use warmup::{WarmupStep, ramp as warmup_ramp};
pub use workout::{DerivedFrom, MesocycleId, PrescribedWorkout};
