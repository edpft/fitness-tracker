//! The gym session that was performed: what was done in one visit.
//!
//! **The session is the unit, and Hevy's record boundary is not it.** The
//! operator, 2026-09-08:
//!
//! > the domain model is the session, which could contain more than one record
//! > from a provider or contain data from more than one provider endpoint
//!
//! > on the gym side, there was a time when I used different Hevy routines to
//! > programme different parts of my workout so I could compose them. they were
//! > all still part of the same gym session.
//!
//! Twenty-one of his 140 training days carry more than one Hevy record, three of
//! them four. Without this type every session count, frequency figure and streak
//! over those days is inflated (§ 10), and `compare` refuses them outright as an
//! ambiguous day.
//!
//! **No roles, and no variants.** This is where the gym and cycling part
//! company, and the reason is what each source states. Peloton names a class's
//! kind — `series_id` makes a warm-up a warm-up and `class_type_ids` makes a
//! cool-down a cool-down — so [`crate::cycling::PerformedSession`] has roles and
//! two variants. Hevy states nothing of the sort: nine keys in the payload, none
//! naming a kind, and `routine_id` null on 155 of the operator's 167 records
//! because he deleted the routines. Asked directly, he was explicit: *"there are
//! no roles, they are single gym sessions split across multiple Hevy routines
//! (even though those routines no longer exist, because I deleted them)."*
//!
//! So a gym session is its workouts in the order they were performed, and
//! nothing else. Inventing roles from routine titles would be this layer reading
//! copy as a statement — the mistake [`crate::cycling`] deliberately avoided.
//!
//! **This is the performed side.** [`crate::prescription`] holds what was
//! prescribed, and § 11 keeps the two apart.

use std::fmt;

use crate::landing::{LandingRecordId, SourceRecordId};
use crate::normalised::{NormalisedEntity, StartedAt};
use crate::sequence::NonEmpty;

use super::workout::{GymWorkout, PerformedExercise, WorkoutItem};

/// A session performed in a gym.
///
/// One or more [`GymWorkout`]s, in the order they were performed. A session of
/// one is the ordinary case — 119 of the operator's 140 days — and is the
/// degenerate case rather than a different kind of thing, which is why there is
/// no variant for it.
///
/// The workouts stay real. A Hevy workout has its own start, its own provenance
/// and its own [`GymWorkout::performed_against`], and flattening them into one
/// item sequence would lose which prescription each part answered. This is the
/// same call [`crate::cycling::PerformedSession`] made for rides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerformedGymSession {
    workouts: NonEmpty<GymWorkout>,
}

impl PerformedGymSession {
    /// The session those workouts make, in the order given.
    ///
    /// The order is the caller's, and it is the order performed: grouping sorts
    /// by start before it splits, so a session's parts arrive in sequence.
    pub const fn new(workouts: NonEmpty<GymWorkout>) -> Self {
        Self { workouts }
    }

    /// The session's workouts, in the order they were performed.
    ///
    /// Non-empty as a type rather than as a promise, so reading the first one
    /// needs no index and can raise no panic (§ 26).
    pub const fn workouts(&self) -> &NonEmpty<GymWorkout> {
        &self.workouts
    }

    /// When the session started, which is when its first workout did.
    pub const fn started_at(&self) -> &StartedAt {
        self.workouts.first().started_at()
    }

    /// The landing record the session is identified by: its first workout's.
    ///
    /// A session is ours rather than the source's — Hevy names each workout and
    /// names no session — so there is no source identifier to key on. The first
    /// workout's landing record is stable, unique to the session, and already
    /// the thing a refusal points at.
    pub const fn landed_as(&self) -> LandingRecordId {
        self.workouts.first().landed_as()
    }

    /// Every item performed, across the session's workouts, in order.
    ///
    /// **The join is invisible here on purpose.** A session split across four
    /// routines was one sequence of items as it was performed; which routine
    /// each came from is provenance, not shape, and a caller reading the session
    /// as a shape must not have to know where the seams were.
    pub fn items(&self) -> impl Iterator<Item = &WorkoutItem> {
        self.workouts
            .iter()
            .flat_map(|workout| workout.items().iter())
    }

    /// The first item performed. There is always one: a workout's items are
    /// non-empty and so are a session's workouts.
    pub const fn first_item(&self) -> &WorkoutItem {
        self.workouts.first().items().first()
    }

    /// Every performed exercise, flattened across the session's workouts in the
    /// order they were performed.
    pub fn exercises(&self) -> impl Iterator<Item = &PerformedExercise> {
        self.items().flat_map(WorkoutItem::exercises)
    }

    /// How many sets the session held, over all its workouts.
    pub fn set_count(&self) -> usize {
        self.exercises()
            .map(PerformedExercise::set_count)
            .sum::<usize>()
    }

    /// How many Hevy records this session was split across.
    pub const fn part_count(&self) -> usize {
        self.workouts.count()
    }
}

impl NormalisedEntity for PerformedGymSession {
    /// Every workout the session holds.
    ///
    /// So a retraction of any one part withdraws the whole session: a session
    /// missing one of its parts is not that session, and asserting it would be
    /// this layer inventing what the source did not say.
    fn composes(&self) -> Vec<&SourceRecordId> {
        self.workouts
            .iter()
            .map(GymWorkout::source_record_id)
            .collect()
    }
}

impl fmt::Display for PerformedGymSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {} workouts, {} sets",
            self.started_at(),
            self.part_count(),
            self.set_count()
        )
    }
}
