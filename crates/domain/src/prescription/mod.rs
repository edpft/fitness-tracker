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
pub mod block;
pub mod ladder;
pub mod linear;
pub mod parameters;
pub mod prilepin;
pub mod progression;
pub mod project;
pub mod repmax;
pub mod schedule;
pub mod shape;
pub mod steps;
pub mod succession;
pub mod target;
pub mod workout;

pub use anchor::{Anchor, AnchorProvenance, Entry, InvalidAnchor, UnknownProvenance};
pub use ladder::{InvalidLadder, Ladder, Opening};
pub use linear::{
    Fill, InconsistentProgramme, Position, Primary, PrimaryPattern, Programme, SlotContent,
    SlotFills, StaticFill,
};
pub use parameters::{
    AccessoryScheme, BackOff, GenerationParameters, InvalidPercentage, Percentage, ResetProtocol,
    Scales, TopSetReps, WarmupStep,
};
pub use progression::{GatingTopSet, Progress, Reset, progress_after};
pub use project::{Divergence, ItemPosition, Projection, ProjectionGap, project, satisfies};
pub use repmax::rep_max;
pub use schedule::{
    Calendar, Interruptions, InvalidCalendar, InvalidWeek, NoWeekdays, NotScheduled, PerRole,
    SessionRole, Skip, UnknownSessionRole, WeekIndex, WeekKind, Weekdays,
};
pub use shape::{
    Block, PrescribedExercise, PrescribedItem, PrescribedSuperset, SlotId, SupersetMember,
    UnknownSlot, WorkoutShape,
};
pub use steps::{InvalidLoadSteps, LoadSteps, Step};
pub use succession::{InvalidProgrammeName, ProgrammeName, ProgrammeWindow};
pub use target::{EmptyRange, Prescribed, PrescribedSet, Target};
pub use workout::{PrescribedWorkout, ProgrammeId};
