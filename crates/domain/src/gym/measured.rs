//! The gym session a watch recorded: its heart rate, and what it made of the
//! exercises.
//!
//! **Two parts, and either can stand without the other.** The operator,
//! 2026-09-18:
//!
//! > for sessions where I recorded by heart rate via my watch and my exercise
//! > performance via the Hevy app, you'll see a Hevy workout with exercise data
//! > and no heart and a Garmin strength training session with, probably, 1 set
//! > of something random, and some heart rate data. […] I have never entered
//! > the same set, reps, weight data in Hevy and Garmin.
//!
//! The sentence elided between the two is his instruction to split the heart
//! rate from the exercise data here, in the normalisation layer, which is what
//! this type is.
//!
//! So the two never give competing accounts of one thing, and 179 of his gym
//! activities — every one from 2015 and 2016 among them — carry heart rate and
//! no sets at all. [`Recorded`] is what makes that a session rather than a
//! session with a hole in it.
//!
//! **Nothing here is an account of the exercises to rival a log's.** What the
//! watch measured is heart rate, rep counts and the clock; what it *proposed*
//! is the movement, and it proposed one for every set it recorded, right or
//! wrong. Garmin calls the operator's three sets of ten at 70 kg on 2019-03-07
//! `BARBELL_DEADLIFT`, and his own sheet for that block calls them Romanian
//! deadlifts. Every entry the source serves carries a probability, and no field
//! anywhere is the operator saying what he did.
//!
//! That is why [`GuessedExercise`] exists rather than a plain exercise on the
//! set. A canonical session (#247) takes the movement from a source that knows
//! it — Hevy, the gym's log, a spreadsheet — and this from the 132 sessions
//! where nothing else recorded anything at all. Settled with the operator,
//! 2026-09-28: keep the watch's top guess, marked as the watch's.
//!
//! **A flat sequence of sets, because that is what the watch records.** There
//! are no exercise entries to group them under and no supersets: a watch sees
//! a set start, counts reps and sees it end. Grouping consecutive sets it
//! guessed the same movement for would be inventing structure out of a guess.

use std::fmt;

use crate::{
    landing::{LandingRecordId, Provenance, SourceRecordId},
    measure::{Duration, HeartRateSummary, RepCount},
    normalised::{NormalisedEntity, StartedAt},
    sequence::NonEmpty,
};

use super::{exercise::RepsExercise, load::Load};

/// The movement a set was of, as a watch's classifier proposed it.
///
/// Never an observation, whichever variant it is. The name is here so that a
/// reader of a set cannot mistake the proposal for a record of what was done,
/// which an `Option<RepsExercise>` on the set would invite.
///
/// **What the source proposed and what was worked out from it are different
/// variants**, because only the first is something the source said. The
/// operator, 2026-09-28: *"if an Unknown appears between two named exercises,
/// with the same reps and loads, we can assume it is the same exercise."* Where
/// that holds, [`Self::FromItsRun`] says the movement came from the sets either
/// side rather than from the classifier, so nothing downstream has to take a
/// translator's inference for a source's claim.
///
/// **A grouping the classifier could not narrow is not a third kind of
/// answer.** Garmin states a movement, or a category of movements —
/// `BENCH_PRESS` with nothing under it — or `UNKNOWN`. This vocabulary has one
/// level and no families (#93), so the adapter resolves a category to the
/// movement it means in this record, or to nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuessedExercise {
    /// The classifier proposed this movement for this set.
    Proposed(RepsExercise),
    /// The classifier proposed nothing for this set, and the run of sets it
    /// belongs to — one identical load for one identical rep count, performed
    /// back to back — is this movement, on the say-so of a set the classifier
    /// did place.
    ///
    /// Still a guess, and one further from the source than [`Self::Proposed`]:
    /// the movement carried here was the classifier's guess about its own set
    /// before it was carried to this one.
    FromItsRun(RepsExercise),
    /// Neither the classifier nor the run it sits in says what the movement
    /// was. Garmin's `UNKNOWN`, and every term of its own that names no
    /// movement of ours.
    Undetermined,
}

impl GuessedExercise {
    /// The movement guessed, however it was arrived at.
    pub const fn movement(self) -> Option<RepsExercise> {
        match self {
            Self::Proposed(exercise) | Self::FromItsRun(exercise) => Some(exercise),
            Self::Undetermined => None,
        }
    }

    /// Whether the source proposed this movement for this set, or it was
    /// carried from the run.
    pub const fn from_the_source(self) -> bool {
        matches!(self, Self::Proposed(_))
    }
}

impl fmt::Display for GuessedExercise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Proposed(exercise) => write!(f, "{exercise}?"),
            Self::FromItsRun(exercise) => write!(f, "{exercise}? (from its run)"),
            Self::Undetermined => f.write_str("unidentified"),
        }
    }
}

/// One set, as a watch recorded it.
///
/// **Its own start rather than a position**, because that is what makes the set
/// joinable to another source's account of the same session: a watch and a log
/// agree on the clock and on nothing else.
///
/// **Its own set type, for the reason [`super::ManualSet`] has one.** The load
/// may be missing, and missing is not zero — Garmin serves no weight at all for
/// 602 of the operator's active sets and its `-1` sentinel for 101 more, while
/// a zero is a set he did record as carrying nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredSet {
    /// When the set began, by the watch's clock.
    pub at: StartedAt,
    /// What the watch counted.
    pub reps: RepCount,
    /// What the operator entered against it. `None` where the source states
    /// none, which is not a set carrying nothing.
    pub load: Option<Load>,
    /// What the watch made of the movement.
    pub guess: GuessedExercise,
}

impl fmt::Display for MeasuredSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.load {
            Some(load) => write!(f, "{} × {} — {}", load, self.reps, self.guess),
            None => write!(f, "{} reps — {}", self.reps, self.guess),
        }
    }
}

/// What a watch recorded of a session: its heart rate, its sets, or both.
///
/// **A session with neither is not a session**, so there is no fourth variant
/// and nothing validates. It is the one thing this layer must be able to say
/// about a Garmin activity: a strength activity with no heart rate and no sets
/// asserts nothing, and refusing it is § 37 rather than a gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    HeartRate(HeartRateSummary),
    Sets(NonEmpty<MeasuredSet>),
    Both {
        heart_rate: HeartRateSummary,
        sets: NonEmpty<MeasuredSet>,
    },
}

impl Recorded {
    /// Whichever of the two parts the caller wants, absent where the watch
    /// recorded it and present where it did. The split is the point of the
    /// type: a caller after heart rate never has to know whether there were
    /// sets.
    pub const fn heart_rate(&self) -> Option<&HeartRateSummary> {
        match self {
            Self::HeartRate(heart_rate) | Self::Both { heart_rate, .. } => Some(heart_rate),
            Self::Sets(_) => None,
        }
    }

    pub const fn sets(&self) -> Option<&NonEmpty<MeasuredSet>> {
        match self {
            Self::Sets(sets) | Self::Both { sets, .. } => Some(sets),
            Self::HeartRate(_) => None,
        }
    }

    /// The two parts, where a caller wants both and neither is required.
    pub const fn parts(&self) -> (Option<&HeartRateSummary>, Option<&NonEmpty<MeasuredSet>>) {
        (self.heart_rate(), self.sets())
    }
}

/// A gym session a watch recorded.
///
/// Identified by the landing record the activity came from, as every entity in
/// this layer is: two records sharing a source identity are the source
/// contradicting itself, and keying on the source's id would collapse the pair
/// silently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredGymSession {
    started_at: StartedAt,
    duration: Duration,
    recorded: Recorded,
    provenance: Provenance,
    source_record_id: SourceRecordId,
    landed_as: LandingRecordId,
    /// The record the sets came from, where they came from one.
    ///
    /// **A second response about one thing** (§ 3.1): the activity list states
    /// a session's start, duration and heart rate, and its sets are a second
    /// endpoint fetched per activity. Neither response is the other's, so a
    /// refusal or a rebuild can say which of the two it is about.
    sets_landed_as: Option<LandingRecordId>,
}

impl MeasuredGymSession {
    pub const fn new(
        started_at: StartedAt,
        duration: Duration,
        recorded: Recorded,
        provenance: Provenance,
        source_record_id: SourceRecordId,
        landed_as: LandingRecordId,
        sets_landed_as: Option<LandingRecordId>,
    ) -> Self {
        Self {
            started_at,
            duration,
            recorded,
            provenance,
            source_record_id,
            landed_as,
            sets_landed_as,
        }
    }

    pub const fn started_at(&self) -> &StartedAt {
        &self.started_at
    }

    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// What the watch recorded, in the two parts it recorded it in.
    pub const fn recorded(&self) -> &Recorded {
        &self.recorded
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

    pub const fn sets_landed_as(&self) -> Option<LandingRecordId> {
        self.sets_landed_as
    }

    /// How many sets the watch recorded. Zero for a session it recorded only a
    /// heart rate for.
    pub fn set_count(&self) -> usize {
        self.recorded.sets().map_or(0, NonEmpty::count)
    }
}

impl NormalisedEntity for MeasuredGymSession {
    /// The activity, once.
    ///
    /// **Once although it composes two responses**, because Garmin names both
    /// by the same identifier: an activity's sets are served under the
    /// activity's own id, so the two landing records share a
    /// [`SourceRecordId`] and returning it twice would say the entity stands on
    /// two things the source could withdraw separately. It cannot: withdrawing
    /// the activity withdraws its sets.
    fn composes(&self) -> Vec<&SourceRecordId> {
        vec![&self.source_record_id]
    }
}

impl fmt::Display for MeasuredGymSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.started_at)?;
        match self.recorded.heart_rate() {
            Some(heart_rate) => write!(f, " — {heart_rate}")?,
            None => f.write_str(" — no heart rate")?,
        }
        match self.set_count() {
            0 => f.write_str(", no sets"),
            count => write!(f, ", {count} sets"),
        }
    }
}
