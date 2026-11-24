//! The gym session that happened, however many sources recorded it.
//!
//! The canonical layer's entity (§ II.4), and the operator's ask, 2026-09-28:
//! *"a canonical view of performed workouts that provides the fullest view
//! possible by joining Garmin (Heart Rate and Exercise), Hevy, BTWB, and
//! historical spreadsheet data."* And, rejecting the first attempt at it:
//! *"I was looking for consolidation and merging, taking the best of each
//! source to produce the fullest picture possible."*
//!
//! **So it is a merge, and it holds its own exercises and sets.** One session
//! per visit, with its own ordered items, and each field of each set taken
//! from whichever normalised session recorded it and naming that session
//! ([`Attributed`]). It does not name its parts and send the reader elsewhere;
//! that was #297, and it was withdrawn.
//!
//! **Two sessions in the record pay for the merge on their own.**
//!
//! 2018-04-28. `Training Log` holds five sets: a deadlift, a bench press and
//! three squats, with loads. Garmin holds fifteen — the same squats exactly,
//! the earlier deadlift and bench sets the sheet summarised away, and two
//! whole exercises the sheet never recorded, seated and standing calf raises.
//! The sheet contributes the movement names it confirms; the watch contributes
//! ten sets and two exercises.
//!
//! 2020-10-09. `the_beginner_prescription.xlsx` holds eighteen sets with the
//! right movements and reps and **not one load** — every `kg` cell was a
//! formula whose lookup came back empty — while Garmin holds the same eighteen
//! with every load, the clock, and a warm-up the sheet missed. The sheet is a
//! performance and not a plan: it derives a session only where the operator
//! left a positive sign he did it, and here that is the reps in reserve he
//! typed against two of the deadlift sets. Merged, the session goes from
//! unusable to complete, and the merging happens **inside a set** — reps from
//! the sheet, load from the watch — which is why [`Attributed`] is per field.
//!
//! **The one difference in the corpus is not a disagreement.** On 2019-03-14 a
//! sheet has the first bench set at 47.5 kg and the watch has it at 48. The
//! operator, 2026-09-28: *"Garmin didn't let you record decimal weights! You
//! enter the load via a dial on the watch face, integer only … Barbell based
//! gym lifts are almost always multiples of 2.5kg."* § 10 was amended for
//! exactly this: the canonical set holds the normalised session that could express the
//! value, and the other stands unchanged below.
//!
//! **What it does not hold.** No set count and no volume: both are functions
//! of what is here, and § II.4 keeps derived figures at the analytical layer.
//! No prescription either — § 11 stores prescribed and performed separately,
//! and what a workout was performed against is the normalised workout's.

use std::collections::BTreeSet;
use std::fmt;

use jiff::Timestamp;

use crate::canonical::{Attributed, NormalisedSessionId, Occurred, TakenFrom};
use crate::landing::LandingStream;
use crate::measure::{Distance, Duration, MeasuredHeartRate, PositiveDuration, RepCount};
use crate::normalised::StartedAt;
use crate::sequence::{AtLeastTwo, NonEmpty};

use super::{
    exercise::{DistanceExercise, DurationExercise, RepsExercise},
    identity::SourceSet,
    intensity::Rir,
    load::Load,
    measured::GuessedExercise,
    outcome::Performed,
    set::SetKind,
};

/// Which exercise a set was of, and on whose terms.
///
/// **The two arms are different kinds of claim, which is why they are not one
/// field.** A log says what the operator did; a watch's classifier says what
/// it made of what he did, and it says it for every set it recorded, right or
/// wrong. Flattening them would let a reader take `BARBELL_DEADLIFT` for the
/// operator's word when his own sheet for that block says Romanian deadlift.
///
/// **The guess survives into this layer** rather than being resolved away. The
/// operator, 2026-09-29, on the watch's guesses where nothing else covers the
/// set: *"Yes, take them, though I may be able to match them to our catalogue
/// retrospectively."* A later re-match is then an edit (§ II.2) of the
/// canonical exercise, applied as this layer is rebuilt — not patched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Identified {
    /// A source that records what was done named it: Hevy, the gym's log, a
    /// spreadsheet.
    Recorded(RepsExercise),
    /// Nothing but a watch's classifier placed it, and this is as far as it
    /// got — including [`GuessedExercise::Undetermined`], where it got
    /// nowhere and the sets are real all the same.
    Guessed(GuessedExercise),
}

impl Identified {
    /// The exercise, where anything named one. `None` where only a movement
    /// was described, or nothing was.
    pub const fn exercise(self) -> Option<RepsExercise> {
        match self {
            Self::Recorded(exercise) => Some(exercise),
            Self::Guessed(guess) => guess.exercise(),
        }
    }

    /// Whether a source that records what was done named it.
    pub const fn is_recorded(self) -> bool {
        matches!(self, Self::Recorded(_))
    }

    /// The stable key for which kind of claim this is. Persisted.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Recorded(_) => "recorded",
            Self::Guessed(_) => "guessed",
        }
    }
}

impl fmt::Display for Identified {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Recorded(exercise) => write!(f, "{exercise}"),
            Self::Guessed(guess) => write!(f, "{guess}"),
        }
    }
}

/// One set of a canonical session, field by field.
///
/// **Every field but the outcome is optional, and every one of them names its
/// normalised session.** No source records all of them: Hevy has reps in reserve and no
/// rest at all, the sheets have rest and sometimes no load, Beyond The White
/// Board has a count it may leave unstated, and the watch has the clock and
/// nothing else has it. A field no normalised session recorded is absent, which is not
/// the same as zero.
///
/// **The outcome is not optional** because a set nothing can say happened is
/// not a set. Its measure may still be unstated — `Completed(None)` is the
/// gym's log writing "Burpee" in a round and not how many — which is why the
/// measure is inside [`Performed`] rather than beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalSet<M> {
    /// What became of the set, and how much of it where a normalised session said.
    pub outcome: Attributed<Performed<Option<M>>, TakenFrom>,
    /// `None` where no normalised session recorded a load, which is not an unloaded set:
    /// that is `Some` of [`Load::UNLOADED`].
    pub load: Option<Attributed<Load, TakenFrom>>,
    /// When the set began. Only a watch records this, so it is present on the
    /// sessions a watch recorded and absent on the rest.
    pub began: Option<Attributed<StartedAt, TakenFrom>>,
    pub intensity: Option<Attributed<Rir, TakenFrom>>,
    /// Working or warm-up. **Optional here although it is not on a logged
    /// set**, because the watch does not state it: a session only a watch
    /// recorded has sets whose kind nothing says, and defaulting them to
    /// working would file a warm-up as volume.
    pub kind: Option<Attributed<SetKind, TakenFrom>>,
    pub rest_after: Option<Attributed<Duration, TakenFrom>>,
    /// The source sets it was built from, by the sources' own names: what it
    /// is across a rebuild ([`super::identity`]).
    pub sources: BTreeSet<SourceSet>,
}

impl<M> CanonicalSet<M> {
    /// Every normalised session this set drew a field from, each once, in
    /// field order. A field the operator's edit states names none.
    pub fn normalised_sessions(&self) -> Vec<NormalisedSessionId> {
        let mut sessions = Vec::new();
        let fields = [
            self.outcome.normalised(),
            self.load.as_ref().and_then(Attributed::normalised),
            self.began.as_ref().and_then(Attributed::normalised),
            self.intensity.as_ref().and_then(Attributed::normalised),
            self.kind.as_ref().and_then(Attributed::normalised),
            self.rest_after.as_ref().and_then(Attributed::normalised),
        ];
        for session in fields.into_iter().flatten() {
            if !sessions.contains(&session) {
                sessions.push(session);
            }
        }
        sessions
    }
}

impl<M: fmt::Display> fmt::Display for CanonicalSet<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(load) = &self.load {
            write!(f, "{} × ", load.value())?;
        }
        match self.outcome.value() {
            Performed::Completed(Some(measure)) => write!(f, "{measure}")?,
            Performed::Completed(None) => f.write_str("done")?,
            Performed::Failed => f.write_str("failed")?,
        }
        if let Some(intensity) = &self.intensity {
            write!(f, " @ {} in reserve", intensity.value())?;
        }
        if self.kind.as_ref().map(Attributed::copied) == Some(SetKind::Warmup) {
            f.write_str(" (warmup)")?;
        }
        Ok(())
    }
}

/// One exercise of a canonical session, and the sets performed of it.
///
/// Which variant this is fixes the measure, exactly as it does one layer down,
/// so a set and its exercise cannot disagree and nothing validates the
/// pairing.
///
/// **Only the reps arm can be a guess**, because only a watch guesses and a
/// watch counts repetitions. A duration or a distance exercise is here because
/// a log recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalExercise {
    ForReps {
        identified: Attributed<Identified, TakenFrom>,
        sets: NonEmpty<CanonicalSet<RepCount>>,
    },
    ForDuration {
        exercise: Attributed<DurationExercise, TakenFrom>,
        sets: NonEmpty<CanonicalSet<Duration>>,
    },
    ForDistance {
        exercise: Attributed<DistanceExercise, TakenFrom>,
        sets: NonEmpty<CanonicalSet<Distance>>,
    },
}

impl CanonicalExercise {
    pub const fn measure(&self) -> &'static str {
        match self {
            Self::ForReps { .. } => "reps",
            Self::ForDuration { .. } => "duration",
            Self::ForDistance { .. } => "distance",
        }
    }

    /// The exercise's stable key, where anything named one. `None` where only
    /// a watch placed it and it described a movement without naming an
    /// exercise, or described nothing.
    pub const fn exercise_key(&self) -> Option<&'static str> {
        match self {
            Self::ForReps { identified, .. } => match identified.copied().exercise() {
                Some(exercise) => Some(exercise.as_str()),
                None => None,
            },
            Self::ForDuration { exercise, .. } => Some(exercise.copied().as_str()),
            Self::ForDistance { exercise, .. } => Some(exercise.copied().as_str()),
        }
    }

    /// The normalised session the exercise's own identity came from, or
    /// [`None`] where the operator's edit named it.
    pub const fn identified_by(&self) -> Option<NormalisedSessionId> {
        match self {
            Self::ForReps { identified, .. } => identified.normalised(),
            Self::ForDuration { exercise, .. } => exercise.normalised(),
            Self::ForDistance { exercise, .. } => exercise.normalised(),
        }
    }

    pub const fn set_count(&self) -> usize {
        match self {
            Self::ForReps { sets, .. } => sets.count(),
            Self::ForDuration { sets, .. } => sets.count(),
            Self::ForDistance { sets, .. } => sets.count(),
        }
    }

    /// Each set's source sets, sets in order.
    pub fn set_sources(&self) -> Vec<&BTreeSet<SourceSet>> {
        match self {
            Self::ForReps { sets, .. } => sets.iter().map(|set| &set.sources).collect(),
            Self::ForDuration { sets, .. } => sets.iter().map(|set| &set.sources).collect(),
            Self::ForDistance { sets, .. } => sets.iter().map(|set| &set.sources).collect(),
        }
    }

    /// When each of its sets began, where a watch timed it.
    pub fn began(&self) -> Vec<Timestamp> {
        fn of<M>(set: &CanonicalSet<M>) -> Option<Timestamp> {
            set.began.as_ref().map(|began| began.value().instant())
        }
        match self {
            Self::ForReps { sets, .. } => sets.iter().filter_map(of).collect(),
            Self::ForDuration { sets, .. } => sets.iter().filter_map(of).collect(),
            Self::ForDistance { sets, .. } => sets.iter().filter_map(of).collect(),
        }
    }

    /// Every normalised session this exercise and its sets drew a field from, each once.
    pub fn normalised_sessions(&self) -> Vec<NormalisedSessionId> {
        let mut sessions: Vec<NormalisedSessionId> = self.identified_by().into_iter().collect();
        let sets: Vec<Vec<NormalisedSessionId>> = match self {
            Self::ForReps { sets, .. } => {
                sets.iter().map(CanonicalSet::normalised_sessions).collect()
            }
            Self::ForDuration { sets, .. } => {
                sets.iter().map(CanonicalSet::normalised_sessions).collect()
            }
            Self::ForDistance { sets, .. } => {
                sets.iter().map(CanonicalSet::normalised_sessions).collect()
            }
        };
        for session in sets.into_iter().flatten() {
            if !sessions.contains(&session) {
                sessions.push(session);
            }
        }
        sessions
    }
}

impl fmt::Display for CanonicalExercise {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForReps { identified, .. } => write!(f, "{}", identified.value())?,
            Self::ForDuration { exercise, .. } => write!(f, "{}", exercise.value())?,
            Self::ForDistance { exercise, .. } => write!(f, "{}", exercise.value())?,
        }
        write!(f, " × {}", self.set_count())
    }
}

/// One position in a canonical session's order: an exercise, or exercises
/// performed back to back.
///
/// A superset is a thing a source recorded, not a thing the merge works out:
/// Hevy records 360 superset items of 807 and the gym's log 55 of 99, and no
/// spreadsheet or watch records one at all.
///
/// **Both are boxed**, because a canonical exercise is large: every field of
/// every set carries the normalised session it came from, each set's clock
/// carries the zone it was read in, and each set the source sets it was built
/// from. [`AtLeastTwo`] holds its first two members inline, so an unboxed
/// superset would make every item the size of two exercises; and once a set
/// named its sources (#893) an unboxed single exercise was most of that again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalItem {
    Exercise(Box<CanonicalExercise>),
    Superset(Box<AtLeastTwo<CanonicalExercise>>),
}

impl CanonicalItem {
    /// One exercise, performed on its own.
    pub fn exercise(exercise: CanonicalExercise) -> Self {
        Self::Exercise(Box::new(exercise))
    }

    /// Every exercise in this item, in order: one, or a superset's members.
    pub fn exercises(&self) -> Box<dyn Iterator<Item = &CanonicalExercise> + Send + '_> {
        match self {
            Self::Exercise(exercise) => Box::new(std::iter::once(exercise.as_ref())),
            Self::Superset(members) => Box::new(members.iter()),
        }
    }
}

/// One gym session that happened, however many sources recorded it.
///
/// **Exercises are required and the heart rate is not.** The operator,
/// 2026-10-01: *"A canonical gym session must have exercises and may have heart
/// rate data. Heart rate only isn't a meaningful gym session."* So an activity
/// that holds a heart rate and nothing else — 307 of the operator's 477 Garmin
/// strength activities — is a normalised session and no canonical one, and
/// there is no variant for it to be the degenerate case of.
///
/// A watch that recorded sets does give exercises, as [`Identified::Guessed`],
/// which is why the rule costs the record only the activities where the watch
/// classified nothing at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalGymSession {
    /// When it started, at the precision its sources knew it to, and which
    /// normalised session said so, or the edit.
    occurred: Attributed<Occurred, TakenFrom>,
    exercises: NonEmpty<CanonicalItem>,
    /// What the heart did while they were performed, where a watch recorded it.
    ///
    /// **One normalised session's whole, never merged.** Only a watch records a heart
    /// rate, it is method-dependent (§ 6), and § 10 keeps method-dependent
    /// quantities out of the merge: there is no single value two normalised sessions of it
    /// could reduce to.
    heart_rate: Option<Attributed<MeasuredHeartRate>>,
    /// How long the visit lasted, where it started at an instant and a
    /// source said how long it took. Hevy and a watch both do; a sheet and
    /// the gym's log do not.
    ///
    /// **A start and a duration, never an end** (operator, 2026-11-24:
    /// *"internally, we should always store a start and a non-negative
    /// duration, in the UI we can deal in start and end times"*). Attributed
    /// to wherever its end was taken from, since that is what the operator
    /// chooses.
    lasted: Option<Attributed<PositiveDuration, TakenFrom>>,
    /// Each source's own account of when the visit started and ended, so an
    /// edit can take either from the one the operator chooses (#894).
    accounts: Vec<Account>,
}

/// One source's account of when a visit started and ended.
///
/// **Each account is a floor on the visit**: it says the operator was
/// training over the span it covers and nothing about the rest. The
/// canonical session's own start and end are merged from them; these are
/// kept so the operator can choose one source's over the merge (operator,
/// 2026-11-23: *"choose the source of the start or end time or provide my
/// own"*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub stream: LandingStream,
    pub normalised: NormalisedSessionId,
    pub occurred: Occurred,
    /// How long it said the visit lasted, where it said: what the source
    /// recorded, with nothing trimmed.
    pub duration: Option<PositiveDuration>,
}

impl Account {
    /// When it says the visit ended, where it says when it started and for
    /// how long.
    #[must_use]
    pub fn ended(&self) -> Option<Timestamp> {
        ended(self.occurred.instant()?.instant(), self.duration?)
    }
}

/// `duration` after `start`.
fn ended(start: Timestamp, duration: PositiveDuration) -> Option<Timestamp> {
    let seconds = i64::try_from(duration.as_seconds()).ok()?;
    start
        .checked_add(jiff::SignedDuration::from_secs(seconds))
        .ok()
}

/// From `start` to `end`, where that is any time at all.
#[must_use]
pub fn between(start: Timestamp, end: Timestamp) -> Option<PositiveDuration> {
    let seconds = u64::try_from(end.as_second() - start.as_second()).ok()?;
    PositiveDuration::from_seconds(seconds).ok()
}

/// Why a start and an end cannot be one session's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidBounds {
    #[error("it would end before it started")]
    EndsBeforeItStarts,
    #[error("it would end with no time to start from")]
    EndsWithoutAStart,
    #[error("it would start on another day")]
    MovesDay,
    #[error("a set the watch timed would fall outside it")]
    LeavesOutASet,
}

impl CanonicalGymSession {
    pub const fn new(
        occurred: Attributed<Occurred, TakenFrom>,
        exercises: NonEmpty<CanonicalItem>,
        heart_rate: Option<Attributed<MeasuredHeartRate>>,
        lasted: Option<Attributed<PositiveDuration, TakenFrom>>,
        accounts: Vec<Account>,
    ) -> Self {
        Self {
            occurred,
            exercises,
            heart_rate,
            lasted,
            accounts,
        }
    }

    /// When it happened, at the precision its sources knew it to.
    pub const fn occurred(&self) -> &Occurred {
        self.occurred.value()
    }

    /// When it started, and where that was taken from.
    pub const fn started(&self) -> &Attributed<Occurred, TakenFrom> {
        &self.occurred
    }

    /// How long it lasted, and where its end was taken from.
    pub const fn lasted(&self) -> Option<&Attributed<PositiveDuration, TakenFrom>> {
        self.lasted.as_ref()
    }

    /// When it ended, and where that was taken from: its start and how long
    /// it lasted, read as a time for the page.
    pub fn ended(&self) -> Option<Attributed<Timestamp, TakenFrom>> {
        let lasted = self.lasted.as_ref()?;
        let end = ended(self.occurred().instant()?.instant(), lasted.copied())?;
        Some(Attributed::new(end, lasted.taken_from()))
    }

    /// Each source's own start and end.
    pub fn accounts(&self) -> &[Account] {
        &self.accounts
    }

    /// How long it lasted.
    pub fn length(&self) -> Option<PositiveDuration> {
        self.lasted.as_ref().map(Attributed::copied)
    }

    pub const fn heart_rate(&self) -> Option<&Attributed<MeasuredHeartRate>> {
        self.heart_rate.as_ref()
    }

    /// The same session started as `started` says and lasting as `lasted`
    /// does, with its heart rate read only between its start and its end.
    ///
    /// # Errors
    ///
    /// [`InvalidBounds`] where the two cannot be one session's: a length
    /// with no instant to start from, a start on another day, or bounds that
    /// leave out a set the watch timed.
    pub fn bounded(
        &self,
        started: Attributed<Occurred, TakenFrom>,
        lasted: Option<Attributed<PositiveDuration, TakenFrom>>,
    ) -> Result<Self, InvalidBounds> {
        if started.value().day() != self.occurred().day() {
            return Err(InvalidBounds::MovesDay);
        }
        let start = started.value().instant().map(StartedAt::instant);
        let end = match &lasted {
            Some(lasted) => {
                let start = start.ok_or(InvalidBounds::EndsWithoutAStart)?;
                Some(ended(start, lasted.copied()).ok_or(InvalidBounds::EndsBeforeItStarts)?)
            }
            None => None,
        };
        let began = self.exercises().flat_map(CanonicalExercise::began);
        for set in began {
            let before = start.is_some_and(|start| set < start);
            let after = end.is_some_and(|end| set > end);
            if before || after {
                return Err(InvalidBounds::LeavesOutASet);
            }
        }
        let heart_rate = self.heart_rate.as_ref().map(|heart_rate| {
            let recording = self
                .accounts
                .iter()
                .find(|account| account.normalised == heart_rate.normalised_session());
            let began = recording
                .and_then(|account| account.occurred.instant())
                .map(StartedAt::instant);
            let finished = recording.and_then(Account::ended);
            // Only a bound moved inside the recording reads it again: one
            // outside it, or where the watch put it, cuts nothing, since the
            // watch writes readings a second or so past the length it states.
            match (began, start, end) {
                (Some(began), Some(start), Some(end)) => {
                    let from = if start > began {
                        start.as_second() - began.as_second()
                    } else {
                        i64::MIN
                    };
                    let to = if finished.is_none_or(|finished| end < finished) {
                        end.as_second() - began.as_second()
                    } else {
                        i64::MAX
                    };
                    Attributed::new(
                        heart_rate.value().between(from, to),
                        heart_rate.normalised_session(),
                    )
                }
                _ => heart_rate.clone(),
            }
        });
        Ok(Self {
            occurred: started,
            exercises: self.exercises.clone(),
            heart_rate,
            lasted,
            accounts: self.accounts.clone(),
        })
    }

    /// The items performed, in order. Never empty.
    pub const fn items(&self) -> &NonEmpty<CanonicalItem> {
        &self.exercises
    }

    /// Every exercise, supersets flattened, in the order performed.
    pub fn exercises(&self) -> impl Iterator<Item = &CanonicalExercise> {
        self.exercises.iter().flat_map(CanonicalItem::exercises)
    }

    /// The same session with each exercise, in the order [`Self::exercises`]
    /// gives them, replaced by what `f` makes of it: its items, supersets and
    /// everything else stand as they were.
    #[must_use]
    pub fn map_exercises(
        &self,
        mut f: impl FnMut(&CanonicalExercise) -> CanonicalExercise,
    ) -> Self {
        let exercises = self.exercises.map_indexed(|_, item| match item {
            CanonicalItem::Exercise(exercise) => CanonicalItem::exercise(f(exercise)),
            CanonicalItem::Superset(members) => {
                CanonicalItem::Superset(Box::new(members.map_indexed(|_, member| f(member))))
            }
        });
        Self {
            occurred: self.occurred.clone(),
            exercises,
            heart_rate: self.heart_rate.clone(),
            lasted: self.lasted,
            accounts: self.accounts.clone(),
        }
    }

    /// How many sets the session holds. Never zero.
    pub fn set_count(&self) -> usize {
        self.exercises().map(CanonicalExercise::set_count).sum()
    }

    /// Every normalised session it stands on, each once, in the order its
    /// fields name them.
    ///
    /// § II.4: provenance survives reconciliation, and a canonical entity
    /// always names the normalised entities it stands for. **Derived rather
    /// than held**, because every field already names its normalised session, and
    /// a second list beside them would be a second source of truth for the same
    /// fact — one that could disagree with the fields it summarises.
    ///
    /// Never empty: the exercises are non-empty and each names a normalised
    /// session.
    pub fn stands_on(&self) -> Vec<NormalisedSessionId> {
        let mut sessions = Vec::new();
        let mut push = |session: NormalisedSessionId| {
            if !sessions.contains(&session) {
                sessions.push(session);
            }
        };
        if let Some(heart_rate) = &self.heart_rate {
            push(heart_rate.normalised_session());
        }
        for bound in [
            self.occurred.normalised(),
            self.lasted.and_then(|lasted| lasted.normalised()),
        ]
        .into_iter()
        .flatten()
        {
            push(bound);
        }
        for exercise in self.exercises.iter().flat_map(CanonicalItem::exercises) {
            for session in exercise.normalised_sessions() {
                push(session);
            }
        }
        sessions
    }
}

impl fmt::Display for CanonicalGymSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.occurred())?;
        match &self.heart_rate {
            Some(heart_rate) => write!(f, " — {}", heart_rate.value())?,
            None => f.write_str(" — no heart rate")?,
        }
        write!(
            f,
            ", {} sets from {} normalised sessions",
            self.set_count(),
            self.stands_on().len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::measured::Guess;
    use super::*;
    use crate::canonical::NegativeNormalisedSessionId;
    use crate::measure::{HeartRateSummary, Kg};
    use crate::normalised::{OperatorZone, UnknownTimeZone};
    use jiff::Timestamp;

    fn session(id: i64) -> Result<NormalisedSessionId, NegativeNormalisedSessionId> {
        NormalisedSessionId::try_from(id)
    }

    fn zone() -> Result<OperatorZone, UnknownTimeZone> {
        OperatorZone::try_from("Europe/London".to_owned())
    }

    /// A set of `reps` from `sheet`, with the load from `watch` where one is
    /// given. The shape of 2020-10-09.
    fn set(
        reps: u32,
        sheet: NormalisedSessionId,
        load: Option<(u64, NormalisedSessionId)>,
    ) -> Result<CanonicalSet<RepCount>, crate::measure::InvalidQuantity> {
        Ok(CanonicalSet {
            outcome: Attributed::new(Performed::Completed(Some(RepCount::new(reps)?)), sheet),
            load: load.map(|(grams, watch)| {
                Attributed::new(Load::absolute(Kg::from_grams(grams)), watch)
            }),
            began: None,
            intensity: None,
            kind: None,
            rest_after: None,
            sources: BTreeSet::new(),
        })
    }

    /// 2020-10-09. `the_beginner_prescription.xlsx` holds the movement and the
    /// reps and no load; Garmin holds the load for the same set. One canonical
    /// set holds both, and says which normalised session each came from.
    #[test]
    fn one_set_merges_two_normalised_sessions_field_by_field() {
        let sheet = session(1).expect("a normalised session");
        let watch = session(2).expect("a normalised session");
        let deadlift = set(4, sheet, Some((63_000, watch))).expect("a set");

        assert_eq!(deadlift.outcome.normalised(), Some(sheet));
        assert_eq!(
            deadlift.load.as_ref().and_then(Attributed::normalised),
            Some(watch),
            "the load is the watch's, because the sheet's formula came back empty"
        );
        assert_eq!(deadlift.normalised_sessions(), vec![sheet, watch]);
    }

    /// The session that set belongs to stands on both accounts, each once,
    /// although neither recorded the whole of it.
    #[test]
    fn a_session_stands_on_every_normalised_session_its_fields_name() {
        let sheet = session(1).expect("a normalised session");
        let watch = session(2).expect("a normalised session");
        let exercise = CanonicalExercise::ForReps {
            identified: Attributed::new(Identified::Recorded(RepsExercise::DeadliftBarbell), sheet),
            sets: NonEmpty::of(
                set(4, sheet, Some((63_000, watch))).expect("a set"),
                vec![set(4, sheet, Some((65_000, watch))).expect("a set")],
            ),
        };
        let session = CanonicalGymSession::new(
            crate::canonical::Attributed::new(
                Occurred::On("2020-10-09".parse().expect("a day")),
                crate::canonical::NormalisedSessionId::FIRST,
            ),
            NonEmpty::of(CanonicalItem::exercise(exercise), vec![]),
            None,
            None,
            Vec::new(),
        );

        assert_eq!(session.set_count(), 2);
        assert_eq!(session.stands_on(), vec![sheet, watch]);
    }

    /// A session only a watch recorded is still a session: its exercises are
    /// the watch's guesses, and its heart rate sits beside them.
    #[test]
    fn a_watch_only_session_has_the_watch_s_guesses_as_its_exercises() {
        let watch = session(7).expect("a normalised session");
        let instant: Timestamp = "2015-02-09T18:03:00Z".parse().expect("an instant");
        let exercise = CanonicalExercise::ForReps {
            identified: Attributed::new(
                Identified::Guessed(GuessedExercise::Proposed(Guess::Exercise(
                    RepsExercise::BenchPressBarbell,
                ))),
                watch,
            ),
            sets: NonEmpty::of(set(8, watch, Some((40_000, watch))).expect("a set"), vec![]),
        };
        let session = CanonicalGymSession::new(
            crate::canonical::Attributed::new(
                Occurred::At(StartedAt::new(instant, zone().expect("a zone"))),
                watch,
            ),
            NonEmpty::of(CanonicalItem::exercise(exercise), vec![]),
            Some(Attributed::new(
                MeasuredHeartRate::new(
                    HeartRateSummary::new(
                        "77".parse().expect("a rate"),
                        "133".parse().expect("a rate"),
                    ),
                    None,
                ),
                watch,
            )),
            None,
            Vec::new(),
        );

        assert_eq!(session.set_count(), 1);
        assert!(
            !session
                .exercises()
                .any(|exercise| exercise.exercise_key().is_none())
        );
        assert_eq!(session.stands_on(), vec![watch]);
    }

    /// 2019-03-14's first bench set: 47.5 kg in `Strength training 2019.xlsx`
    /// and 48 on the watch, whose dial takes whole kilogrammes. The canonical
    /// set holds 47.5 and names the sheet; the watch is still the normalised session
    /// for the clock.
    #[test]
    fn a_forced_divergence_resolves_to_the_session_that_could_express_it() {
        let sheet = session(3).expect("a normalised session");
        let watch = session(4).expect("a normalised session");
        let instant: Timestamp = "2019-03-14T07:43:00Z".parse().expect("an instant");
        let bench = CanonicalSet {
            outcome: Attributed::new(
                Performed::Completed(Some(RepCount::new(5).expect("a count"))),
                sheet,
            ),
            load: Some(Attributed::new(
                Load::absolute(Kg::from_grams(47_500)),
                sheet,
            )),
            began: Some(Attributed::new(
                StartedAt::new(instant, zone().expect("a zone")),
                watch,
            )),
            intensity: None,
            kind: Some(Attributed::new(SetKind::Working, sheet)),
            rest_after: None,
            sources: BTreeSet::new(),
        };

        assert_eq!(
            bench.load.as_ref().map(Attributed::copied),
            Some(Load::absolute(Kg::from_grams(47_500)))
        );
        assert_eq!(
            bench.load.as_ref().and_then(Attributed::normalised),
            Some(sheet)
        );
        assert_eq!(
            bench.began.as_ref().and_then(Attributed::normalised),
            Some(watch)
        );
        assert_eq!(bench.normalised_sessions(), vec![sheet, watch]);
    }

    /// A set the watch recorded and could not place is still a set. The reps
    /// and the clock are real; what it was of is not known.
    #[test]
    fn an_unidentified_exercise_still_holds_its_sets() {
        let watch = session(5).expect("a normalised session");
        let exercise = CanonicalExercise::ForReps {
            identified: Attributed::new(Identified::Guessed(GuessedExercise::Undetermined), watch),
            sets: NonEmpty::of(set(10, watch, None).expect("a set"), vec![]),
        };

        assert_eq!(exercise.exercise_key(), None);
        assert_eq!(exercise.set_count(), 1);
        assert_eq!(exercise.identified_by(), Some(watch));
        assert_eq!(exercise.normalised_sessions(), vec![watch]);
    }
}
