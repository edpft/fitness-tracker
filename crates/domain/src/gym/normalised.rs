//! A gym session as one source recorded it, in the shape the canonical layer
//! merges it in.
//!
//! **Already attributed, and already in the canonical shape.** Each of these is
//! one account of one visit, and every field of it names the normalised session
//! it came from — itself. The merge is then a fold over several of them rather
//! than a translation from three different set types, and the matching never
//! learns that Hevy's sets carry a load where a sheet's may not.
//!
//! **It is a projection, not the normalised entity.** [`PerformedGymSession`],
//! [`ManualGymSession`] and [`MeasuredGymSession`] are the entities, and each
//! carries identity the canonical layer does not record: a watch session's
//! three landing records, a sheet session's file paths and per-set cells, a
//! Hevy session's event provenance. § II.4 keeps provenance by naming the
//! normalised session, which is the one thing here — so reconstructing the rest
//! in order to drop it would be work with no reader, as
//! [`crate::analytical`]'s weigh-ins already decided for the same reason.
//!
//! [`PerformedGymSession`]: super::PerformedGymSession
//! [`ManualGymSession`]: super::ManualGymSession
//! [`MeasuredGymSession`]: super::MeasuredGymSession

use crate::canonical::{Attributed, NormalisedSessionId, Occurred};
use crate::measure::{PositiveDuration, RepCount};
use crate::sequence::NonEmpty;

use super::{
    canonical::{CanonicalExercise, CanonicalItem, CanonicalSet, Identified},
    measured::{GuessedExercise, MeasuredHeartRate, MeasuredSet},
    outcome::Performed,
};

/// What made a record, which is what decides whether two figures disagree.
///
/// **Not which source, and not how many there were.** § 10 keeps source
/// ranking out of this: two logs differing is settled by what a third account
/// corroborates. What this answers is narrower — whether a figure could be a
/// coarser account of one the other holds exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Recorder {
    /// The operator wrote it down or typed it in: Hevy, the gym's log, a
    /// spreadsheet.
    Operator,
    /// A watch recorded it.
    ///
    /// **Its loads are whole kilogrammes or whole pounds.** The operator,
    /// 2026-10-03: *"the watch input only allows for integers, kg or lbs, if
    /// Garmin has a decimal kg value I must have updated it after the fact.
    /// So, 43kg Vs 42.5kg is no disagreement, it's the same value at different
    /// levels of resolution."* So a watch's 43 against a log's 42.5 is one
    /// value at two resolutions and the finer stands — the same call
    /// [`Occurred`] makes for a day against an instant — and only a difference
    /// the dial could not have produced is a disagreement at all.
    Watch,
}

/// One normalised account of one gym session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalisedGymSession {
    id: NormalisedSessionId,
    recorder: Recorder,
    occurred: Occurred,
    duration: Option<Attributed<PositiveDuration>>,
    heart_rate: Option<Attributed<MeasuredHeartRate>>,
    items: Vec<CanonicalItem>,
}

impl NormalisedGymSession {
    /// `items` are this account's, in the order performed, every field of them
    /// attributed to `id`.
    ///
    /// **Items may be empty**, where a watch recorded a heart rate and
    /// classified no set: 308 of the operator's 478 watch sessions. That is a
    /// normalised session and no canonical one — the operator, 2026-10-01:
    /// *"A canonical gym session must have exercises and may have heart rate
    /// data. Heart rate only isn't a meaningful gym session."* — but its heart
    /// rate still reaches the canonical session its day's log gives exercises
    /// to, which is 123 of his days.
    pub const fn new(
        id: NormalisedSessionId,
        recorder: Recorder,
        occurred: Occurred,
        duration: Option<Attributed<PositiveDuration>>,
        heart_rate: Option<Attributed<MeasuredHeartRate>>,
        items: Vec<CanonicalItem>,
    ) -> Self {
        Self {
            id,
            recorder,
            occurred,
            duration,
            heart_rate,
            items,
        }
    }

    /// The id the canonical layer names this account by.
    pub const fn id(&self) -> NormalisedSessionId {
        self.id
    }

    pub const fn recorder(&self) -> Recorder {
        self.recorder
    }

    /// When it happened, at the precision its source knew it to.
    ///
    /// The matching key, and the reason [`Occurred`] is declared at the
    /// canonical layer rather than here: a day and an instant are two
    /// precisions of one fact, and matching has to compare across them.
    pub const fn occurred(&self) -> &Occurred {
        &self.occurred
    }

    pub const fn duration(&self) -> Option<&Attributed<PositiveDuration>> {
        self.duration.as_ref()
    }

    pub const fn heart_rate(&self) -> Option<&Attributed<MeasuredHeartRate>> {
        self.heart_rate.as_ref()
    }

    /// The items this account holds, in the order performed. Empty where it
    /// holds none.
    pub fn items(&self) -> &[CanonicalItem] {
        &self.items
    }

    /// Whether this account holds anything a canonical session needs
    /// exercises for.
    pub const fn has_items(&self) -> bool {
        !self.items.is_empty()
    }
}

impl NormalisedGymSession {
    /// A log's session: Hevy, the gym's export, a spreadsheet.
    ///
    /// Its items are already in the canonical shape, because a log records
    /// what the operator did in the terms this vocabulary is in — one exercise
    /// it named, and the sets it holds of that exercise.
    pub const fn from_log(
        id: NormalisedSessionId,
        occurred: Occurred,
        duration: Option<Attributed<PositiveDuration>>,
        items: Vec<CanonicalItem>,
    ) -> Self {
        Self::new(id, Recorder::Operator, occurred, duration, None, items)
    }

    /// A watch's session: its heart rate, and the sets it classified itself.
    ///
    /// **Consecutive sets the classifier placed the same way are one
    /// exercise.** A watch records a flat run of sets and names no exercise
    /// boundary, so the boundary has to come from somewhere, and what it
    /// recorded is the order they were performed in: 2018-04-28 is three
    /// deadlifts, then three bench presses, then three back squats, then two
    /// calf-raise exercises, and nothing else in that run is a boundary. Going
    /// by the guess alone rather than by runs would make one exercise of two
    /// visits to the squat rack with another lift between them.
    ///
    /// **A run of [`GuessedExercise::Undetermined`] sets is one exercise too.**
    /// 869 of the operator's watch sets are undetermined and the sets are real
    /// all the same (§ 37); what nothing named is still what was performed in
    /// that part of the session.
    ///
    /// [`GuessedExercise::Undetermined`]: super::GuessedExercise::Undetermined
    pub fn from_watch(
        id: NormalisedSessionId,
        occurred: Occurred,
        duration: Option<Attributed<PositiveDuration>>,
        heart_rate: Option<Attributed<MeasuredHeartRate>>,
        sets: &[MeasuredSet],
    ) -> Self {
        let mut items: Vec<CanonicalItem> = Vec::new();
        let mut run: Vec<CanonicalSet<RepCount>> = Vec::new();
        let mut placed: Option<GuessedExercise> = None;

        for set in sets {
            if placed != Some(set.guess) {
                if let (Some(guess), Some(sets)) = (placed, NonEmpty::new(run).ok()) {
                    items.push(exercise_of(id, guess, sets));
                }
                run = Vec::new();
                placed = Some(set.guess);
            }
            run.push(CanonicalSet {
                outcome: Attributed::new(Performed::Completed(Some(set.reps)), id),
                load: set.load.map(|load| Attributed::new(load, id)),
                began: Some(Attributed::new(set.at.clone(), id)),
                intensity: None,
                kind: None,
                rest_after: None,
            });
        }
        if let (Some(guess), Some(sets)) = (placed, NonEmpty::new(run).ok()) {
            items.push(exercise_of(id, guess, sets));
        }

        Self::new(id, Recorder::Watch, occurred, duration, heart_rate, items)
    }
}

/// One run of a watch's sets, as the exercise it was of.
const fn exercise_of(
    id: NormalisedSessionId,
    guess: GuessedExercise,
    sets: NonEmpty<CanonicalSet<RepCount>>,
) -> CanonicalItem {
    CanonicalItem::Exercise(CanonicalExercise::ForReps {
        identified: Attributed::new(Identified::Guessed(guess), id),
        sets,
    })
}
