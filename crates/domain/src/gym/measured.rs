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
    measure::{Duration, HeartRateSeries, HeartRateSummary, RepCount},
    normalised::{NormalisedEntity, StartedAt},
    sequence::NonEmpty,
};

use super::{
    exercise::{Description, Movement, RepsExercise},
    load::Load,
};

/// What a classifier reached: one of our exercises, or a description of one.
///
/// **Both arms are the source's claim about the movement**, and the difference
/// between them is how far it narrowed. Garmin's `BARBELL_DEADLIFT` names an
/// exercise this vocabulary holds; its bare `ROW` names a movement and stops,
/// and a [`Description`] is what stopping there looks like. Before this arm
/// existed the second case was refused outright, which cost 285 of the
/// operator's sets their load, their reps and their clock over six terms the
/// watch had in fact identified.
///
/// **The movement is total across both**, which is the whole point: a reader
/// after "what movement was this set of" never has to know which arm answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guess {
    /// One of ours, named.
    Exercise(RepsExercise),
    /// A movement, described as far as the source stated it.
    Description(Description),
}

impl Guess {
    /// Which movement the set was of, however far the source narrowed it.
    pub const fn movement(self) -> Movement {
        match self {
            Self::Exercise(exercise) => exercise.movement(),
            Self::Description(description) => description.movement(),
        }
    }

    /// The exercise, where the source named one. `None` where it described a
    /// movement and no more, which is not the same as having said nothing.
    pub const fn exercise(self) -> Option<RepsExercise> {
        match self {
            Self::Exercise(exercise) => Some(exercise),
            Self::Description(_) => None,
        }
    }
}

impl fmt::Display for Guess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exercise(exercise) => write!(f, "{exercise}"),
            Self::Description(description) => write!(f, "{description}"),
        }
    }
}

/// The movement a set was of, as a watch's classifier proposed it.
///
/// Never an observation, whichever variant it is. The name is here so that a
/// reader of a set cannot mistake the proposal for a record of what was done,
/// which an `Option<Guess>` on the set would invite.
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
/// answer**, and it is not nothing either. Garmin states an exercise, or a
/// category of movements — `ROW` with nothing under it — or `UNKNOWN`. The
/// first two are both [`Guess`]; only `UNKNOWN`, and a term naming a movement
/// this vocabulary does not hold, are [`Self::Undetermined`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuessedExercise {
    /// The classifier proposed this for this set.
    Proposed(Guess),
    /// The classifier proposed nothing for this set, and the run of sets it
    /// belongs to — one identical load for one identical rep count, performed
    /// back to back — is this, on the say-so of a set the classifier did place.
    ///
    /// Still a guess, and one further from the source than [`Self::Proposed`]:
    /// what is carried here was the classifier's guess about its own set
    /// before it was carried to this one.
    FromItsRun(Guess),
    /// Neither the classifier nor the run it sits in says what the movement
    /// was. Garmin's `UNKNOWN`, and every term of its own that names no
    /// movement of ours.
    Undetermined,
}

impl GuessedExercise {
    /// What was guessed, however it was arrived at.
    pub const fn guess(self) -> Option<Guess> {
        match self {
            Self::Proposed(guess) | Self::FromItsRun(guess) => Some(guess),
            Self::Undetermined => None,
        }
    }

    /// Which movement was guessed, however it was arrived at and however far
    /// the source narrowed it.
    pub const fn movement(self) -> Option<Movement> {
        match self.guess() {
            Some(guess) => Some(guess.movement()),
            None => None,
        }
    }

    /// The exercise guessed, where one was named. A set described only as a
    /// movement answers `None` here and still answers [`Self::movement`].
    pub const fn exercise(self) -> Option<RepsExercise> {
        match self.guess() {
            Some(guess) => guess.exercise(),
            None => None,
        }
    }

    /// Whether the source proposed this for this set, or it was carried from
    /// the run.
    pub const fn from_the_source(self) -> bool {
        matches!(self, Self::Proposed(_))
    }
}

impl fmt::Display for GuessedExercise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Proposed(guess) => write!(f, "{guess}?"),
            Self::FromItsRun(guess) => write!(f, "{guess}? (from its run)"),
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

/// A session's heart rate as a watch has it: what it states, and the recording
/// itself.
///
/// **Two accounts of one measurement, from two of the source's responses.** The
/// activity list states an average and a highest; the samples they summarise are
/// in the FIT file the third walk lands, at whatever resolution the watch chose
/// to write them. The operator, 2026-09-18: *"heart rate for the canonical
/// session needs per-second samples, not `averageHR`/`maxHR`"*.
///
/// **The summary is kept rather than recomputed from the series.** § II.3 forbids
/// aggregating component observations and says nothing against keeping what a
/// source states — the same call [`HeartRateSummary`] records for a ride's
/// average power. A mean of these samples would be our arithmetic standing in
/// for the watch's, and on an irregular series it is not even the same number.
///
/// **Stated is required and the series is not**, because that is the only
/// combination the record holds: on the operator's corpus the 531 strength
/// activities that state a summary are exactly the 531 whose file holds samples,
/// and the 20 that state none hold no reading anywhere. [`None`] is then a
/// recording that has not landed, or one the watch wrote without a strap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeasuredHeartRate {
    stated: HeartRateSummary,
    series: Option<HeartRateSeries>,
}

impl MeasuredHeartRate {
    pub const fn new(stated: HeartRateSummary, series: Option<HeartRateSeries>) -> Self {
        Self { stated, series }
    }

    /// What the source says the session came to.
    pub const fn stated(&self) -> HeartRateSummary {
        self.stated
    }

    /// The readings themselves, where the recording landed and held any.
    pub const fn series(&self) -> Option<&HeartRateSeries> {
        self.series.as_ref()
    }
}

impl fmt::Display for MeasuredHeartRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.stated)?;
        self.series
            .as_ref()
            .map_or(Ok(()), |series| write!(f, ", {series}"))
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
    HeartRate(MeasuredHeartRate),
    Sets(NonEmpty<MeasuredSet>),
    Both {
        heart_rate: MeasuredHeartRate,
        sets: NonEmpty<MeasuredSet>,
    },
}

impl Recorded {
    /// Whichever of the two parts the caller wants, absent where the watch
    /// recorded it and present where it did. The split is the point of the
    /// type: a caller after heart rate never has to know whether there were
    /// sets.
    pub const fn heart_rate(&self) -> Option<&MeasuredHeartRate> {
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
    pub const fn parts(&self) -> (Option<&MeasuredHeartRate>, Option<&NonEmpty<MeasuredSet>>) {
        (self.heart_rate(), self.sets())
    }
}

/// The landing records one measured session is composed from.
///
/// **Three responses about one thing** (§ 3.1), and Garmin names all three by
/// the same `activityId`: the activity list states the session's start, duration
/// and heart-rate summary, a second endpoint serves its sets, and a third serves
/// the file the watch wrote. None of the three is the other's, so a refusal or a
/// rebuild can say which of them it is about.
///
/// **Two of them are optional and their absence is not a refusal.** 179 of the
/// operator's gym activities carry no sets, and a recording that has not landed
/// is a session whose samples are not here yet rather than a session with a hole
/// in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComposedFrom {
    /// The record the activity's own account came from.
    pub activity: LandingRecordId,
    /// The record its sets came from, where they came from one.
    pub sets: Option<LandingRecordId>,
    /// The record the watch's own recording came from, where it landed.
    pub recording: Option<LandingRecordId>,
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
    landed_as: ComposedFrom,
}

impl MeasuredGymSession {
    pub const fn new(
        started_at: StartedAt,
        duration: Duration,
        recorded: Recorded,
        provenance: Provenance,
        source_record_id: SourceRecordId,
        landed_as: ComposedFrom,
    ) -> Self {
        Self {
            started_at,
            duration,
            recorded,
            provenance,
            source_record_id,
            landed_as,
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

    /// The records this session was composed from.
    pub const fn landed_as(&self) -> ComposedFrom {
        self.landed_as
    }

    /// How many sets the watch recorded. Zero for a session it recorded only a
    /// heart rate for.
    pub fn set_count(&self) -> usize {
        self.recorded.sets().map_or(0, NonEmpty::count)
    }

    /// How many readings its recording holds. Zero where none landed.
    pub fn reading_count(&self) -> usize {
        self.recorded
            .heart_rate()
            .and_then(MeasuredHeartRate::series)
            .map_or(0, |series| series.samples().count())
    }
}

impl NormalisedEntity for MeasuredGymSession {
    /// The activity, once.
    ///
    /// **Once although it composes three responses**, because Garmin names all
    /// of them by the same identifier: an activity's sets and its file are both
    /// served under the activity's own id, so the landing records share a
    /// [`SourceRecordId`] and returning it three times would say the entity
    /// stands on things the source could withdraw separately. It cannot:
    /// withdrawing the activity withdraws its sets and its recording.
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
