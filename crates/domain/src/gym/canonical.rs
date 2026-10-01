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
//! from whichever account recorded it and naming that account
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
//! exactly this: the canonical set holds the account that could express the
//! value, and the other stands unchanged below.
//!
//! **What it does not hold.** No set count and no volume: both are functions
//! of what is here, and § II.4 keeps derived figures at the analytical layer.
//! No prescription either — § 11 stores prescribed and performed separately,
//! and what a workout was performed against is the normalised workout's.

use std::fmt;

use crate::canonical::{Attributed, NormalisedSessionId, Occurred};
use crate::measure::{Distance, Duration, PositiveDuration, RepCount};
use crate::normalised::StartedAt;
use crate::sequence::{AtLeastTwo, NonEmpty};

use super::{
    exercise::{DistanceExercise, DurationExercise, RepsExercise},
    intensity::Rir,
    load::Load,
    measured::{GuessedExercise, MeasuredHeartRate},
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
/// retrospectively."* A later re-match is then an edit overlay (§ II.2)
/// against the normalised set, and this layer is rebuilt from it — not patched.
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
/// account.** No source records all of them: Hevy has reps in reserve and no
/// rest at all, the sheets have rest and sometimes no load, Beyond The White
/// Board has a count it may leave unstated, and the watch has the clock and
/// nothing else has it. A field no account recorded is absent, which is not
/// the same as zero.
///
/// **The outcome is not optional** because a set nothing can say happened is
/// not a set. Its measure may still be unstated — `Completed(None)` is the
/// gym's log writing "Burpee" in a round and not how many — which is why the
/// measure is inside [`Performed`] rather than beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalSet<M> {
    /// What became of the set, and how much of it where an account said.
    pub outcome: Attributed<Performed<Option<M>>>,
    /// `None` where no account recorded a load, which is not an unloaded set:
    /// that is `Some` of [`Load::UNLOADED`].
    pub load: Option<Attributed<Load>>,
    /// When the set began. Only a watch records this, so it is present on the
    /// sessions a watch recorded and absent on the rest.
    pub began: Option<Attributed<StartedAt>>,
    pub intensity: Option<Attributed<Rir>>,
    /// Working or warm-up. **Optional here although it is not on a logged
    /// set**, because the watch does not state it: a session only a watch
    /// recorded has sets whose kind nothing says, and defaulting them to
    /// working would file a warm-up as volume.
    pub kind: Option<Attributed<SetKind>>,
    pub rest_after: Option<Attributed<Duration>>,
}

impl<M> CanonicalSet<M> {
    /// Every account this set drew a field from, each once, in field order.
    pub fn accounts(&self) -> Vec<NormalisedSessionId> {
        let mut accounts = vec![self.outcome.account()];
        let others = [
            self.load.as_ref().map(Attributed::account),
            self.began.as_ref().map(Attributed::account),
            self.intensity.as_ref().map(Attributed::account),
            self.kind.as_ref().map(Attributed::account),
            self.rest_after.as_ref().map(Attributed::account),
        ];
        for account in others.into_iter().flatten() {
            if !accounts.contains(&account) {
                accounts.push(account);
            }
        }
        accounts
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
        identified: Attributed<Identified>,
        sets: NonEmpty<CanonicalSet<RepCount>>,
    },
    ForDuration {
        exercise: Attributed<DurationExercise>,
        sets: NonEmpty<CanonicalSet<Duration>>,
    },
    ForDistance {
        exercise: Attributed<DistanceExercise>,
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

    /// The account the exercise's own identity came from.
    pub const fn identified_by(&self) -> NormalisedSessionId {
        match self {
            Self::ForReps { identified, .. } => identified.account(),
            Self::ForDuration { exercise, .. } => exercise.account(),
            Self::ForDistance { exercise, .. } => exercise.account(),
        }
    }

    pub const fn set_count(&self) -> usize {
        match self {
            Self::ForReps { sets, .. } => sets.count(),
            Self::ForDuration { sets, .. } => sets.count(),
            Self::ForDistance { sets, .. } => sets.count(),
        }
    }

    /// Every account this exercise and its sets drew a field from, each once.
    pub fn accounts(&self) -> Vec<NormalisedSessionId> {
        let mut accounts = vec![self.identified_by()];
        let sets: Vec<Vec<NormalisedSessionId>> = match self {
            Self::ForReps { sets, .. } => sets.iter().map(CanonicalSet::accounts).collect(),
            Self::ForDuration { sets, .. } => sets.iter().map(CanonicalSet::accounts).collect(),
            Self::ForDistance { sets, .. } => sets.iter().map(CanonicalSet::accounts).collect(),
        };
        for account in sets.into_iter().flatten() {
            if !accounts.contains(&account) {
                accounts.push(account);
            }
        }
        accounts
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
/// **The superset is boxed and the single exercise is not**, because
/// [`AtLeastTwo`] holds its first two members inline and a canonical exercise
/// is large: every field of every set carries the account it came from, and
/// each set's clock carries the zone it was read in. Unboxed, every item in
/// the sequence would be the size of two exercises whether or not it held
/// them, and the common item — 447 of Hevy's 807 — holds one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalItem {
    Exercise(CanonicalExercise),
    Superset(Box<AtLeastTwo<CanonicalExercise>>),
}

impl CanonicalItem {
    /// Every exercise in this item, in order: one, or a superset's members.
    pub fn exercises(&self) -> Box<dyn Iterator<Item = &CanonicalExercise> + Send + '_> {
        match self {
            Self::Exercise(exercise) => Box::new(std::iter::once(exercise)),
            Self::Superset(members) => Box::new(members.iter()),
        }
    }
}

/// What the sources between them hold of a session: what was done, what the
/// heart did, or both.
///
/// **A session with neither is not a session**, so there is no fourth variant
/// and nothing validates. It is also what the record is: of 550 gym days, 289
/// are a Garmin activity alone — a heart rate and nothing else — and 48 are a
/// log with no watch beside it.
///
/// **The heart rate is one account's whole.** Only a watch records one, it is
/// method-dependent (§ 6), and § 10 keeps method-dependent quantities out of
/// the merge: there is no second account of it to merge with and never will be
/// one that could be merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parts {
    Exercises(NonEmpty<CanonicalItem>),
    HeartRate(Attributed<MeasuredHeartRate>),
    Both {
        exercises: NonEmpty<CanonicalItem>,
        heart_rate: Attributed<MeasuredHeartRate>,
    },
}

impl Parts {
    pub const fn exercises(&self) -> Option<&NonEmpty<CanonicalItem>> {
        match self {
            Self::Exercises(exercises) | Self::Both { exercises, .. } => Some(exercises),
            Self::HeartRate(_) => None,
        }
    }

    pub const fn heart_rate(&self) -> Option<&Attributed<MeasuredHeartRate>> {
        match self {
            Self::HeartRate(heart_rate) | Self::Both { heart_rate, .. } => Some(heart_rate),
            Self::Exercises(_) => None,
        }
    }
}

/// One gym session that happened, however many sources recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalGymSession {
    occurred: Occurred,
    parts: Parts,
    /// How long the visit took, as whichever account stated it. Hevy and a
    /// watch both do; a sheet and the gym's log do not.
    duration: Option<Attributed<PositiveDuration>>,
}

impl CanonicalGymSession {
    pub const fn new(
        occurred: Occurred,
        parts: Parts,
        duration: Option<Attributed<PositiveDuration>>,
    ) -> Self {
        Self {
            occurred,
            parts,
            duration,
        }
    }

    /// When it happened, at the precision its sources knew it to.
    pub const fn occurred(&self) -> &Occurred {
        &self.occurred
    }

    pub const fn parts(&self) -> &Parts {
        &self.parts
    }

    pub const fn duration(&self) -> Option<&Attributed<PositiveDuration>> {
        self.duration.as_ref()
    }

    /// The items performed, in order. Empty where nothing but a watch's heart
    /// rate records the session, which is 289 of the operator's gym days.
    pub fn items(&self) -> impl Iterator<Item = &CanonicalItem> {
        self.parts.exercises().into_iter().flat_map(NonEmpty::iter)
    }

    /// Every exercise, supersets flattened, in the order performed.
    pub fn exercises(&self) -> impl Iterator<Item = &CanonicalExercise> {
        self.items().flat_map(CanonicalItem::exercises)
    }

    /// How many sets the session holds.
    pub fn set_count(&self) -> usize {
        self.exercises().map(CanonicalExercise::set_count).sum()
    }

    /// Every normalised session it stands on, each once, in the order its
    /// fields name them.
    ///
    /// § II.4: provenance survives reconciliation, and a canonical entity
    /// always names the normalised entities it stands for. **Derived rather
    /// than held**, because every field already names its account and a second
    /// list beside them would be a second source of truth for the same fact —
    /// one that could disagree with the fields it summarises.
    ///
    /// Never empty: a session is its exercises or its heart rate or both, each
    /// of which names an account.
    pub fn stands_on(&self) -> Vec<NormalisedSessionId> {
        let mut accounts = Vec::new();
        let mut push = |account: NormalisedSessionId| {
            if !accounts.contains(&account) {
                accounts.push(account);
            }
        };
        if let Some(heart_rate) = self.parts.heart_rate() {
            push(heart_rate.account());
        }
        if let Some(duration) = &self.duration {
            push(duration.account());
        }
        for exercise in self.exercises() {
            for account in exercise.accounts() {
                push(account);
            }
        }
        accounts
    }
}

impl fmt::Display for CanonicalGymSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.occurred)?;
        match self.parts.heart_rate() {
            Some(heart_rate) => write!(f, " — {}", heart_rate.value())?,
            None => f.write_str(" — no heart rate")?,
        }
        write!(
            f,
            ", {} sets from {} accounts",
            self.set_count(),
            self.stands_on().len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::NegativeNormalisedSessionId;
    use crate::measure::{HeartRateSummary, Kg};
    use crate::normalised::{OperatorZone, UnknownTimeZone};
    use jiff::Timestamp;

    fn account(id: i64) -> Result<NormalisedSessionId, NegativeNormalisedSessionId> {
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
        })
    }

    /// 2020-10-09. `the_beginner_prescription.xlsx` holds the movement and the
    /// reps and no load; Garmin holds the load for the same set. One canonical
    /// set holds both, and says which account each came from.
    #[test]
    fn one_set_merges_two_accounts_field_by_field() {
        let sheet = account(1).expect("an account");
        let watch = account(2).expect("an account");
        let deadlift = set(4, sheet, Some((63_000, watch))).expect("a set");

        assert_eq!(deadlift.outcome.account(), sheet);
        assert_eq!(
            deadlift.load.as_ref().map(Attributed::account),
            Some(watch),
            "the load is the watch's, because the sheet's formula came back empty"
        );
        assert_eq!(deadlift.accounts(), vec![sheet, watch]);
    }

    /// The session that set belongs to stands on both accounts, each once,
    /// although neither recorded the whole of it.
    #[test]
    fn a_session_stands_on_every_account_its_fields_name() {
        let sheet = account(1).expect("an account");
        let watch = account(2).expect("an account");
        let exercise = CanonicalExercise::ForReps {
            identified: Attributed::new(
                Identified::Recorded(RepsExercise::DeadliftBarbell),
                sheet,
            ),
            sets: NonEmpty::of(
                set(4, sheet, Some((63_000, watch))).expect("a set"),
                vec![set(4, sheet, Some((65_000, watch))).expect("a set")],
            ),
        };
        let session = CanonicalGymSession::new(
            Occurred::On("2020-10-09".parse().expect("a day")),
            Parts::Exercises(NonEmpty::of(CanonicalItem::Exercise(exercise), vec![])),
            None,
        );

        assert_eq!(session.set_count(), 2);
        assert_eq!(session.stands_on(), vec![sheet, watch]);
    }

    /// 289 of the operator's gym days are a Garmin activity alone: a heart rate
    /// and nothing else. That is a session, not a session with a hole in it.
    #[test]
    fn a_session_may_be_a_heart_rate_and_nothing_else() {
        let watch = account(7).expect("an account");
        let instant: Timestamp = "2015-02-09T18:03:00Z".parse().expect("an instant");
        let session = CanonicalGymSession::new(
            Occurred::At(StartedAt::new(instant, zone().expect("a zone"))),
            Parts::HeartRate(Attributed::new(
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
        );

        assert_eq!(session.set_count(), 0);
        assert_eq!(session.items().count(), 0);
        assert_eq!(session.stands_on(), vec![watch]);
    }

    /// 2019-03-14's first bench set: 47.5 kg in `Strength training 2019.xlsx`
    /// and 48 on the watch, whose dial takes whole kilogrammes. The canonical
    /// set holds 47.5 and names the sheet; the watch is still the account for
    /// the clock.
    #[test]
    fn a_forced_divergence_resolves_to_the_account_that_could_express_it() {
        let sheet = account(3).expect("an account");
        let watch = account(4).expect("an account");
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
        };

        assert_eq!(
            bench.load.as_ref().map(Attributed::copied),
            Some(Load::absolute(Kg::from_grams(47_500)))
        );
        assert_eq!(bench.load.as_ref().map(Attributed::account), Some(sheet));
        assert_eq!(bench.began.as_ref().map(Attributed::account), Some(watch));
        assert_eq!(bench.accounts(), vec![sheet, watch]);
    }

    /// A set the watch recorded and could not place is still a set. The reps
    /// and the clock are real; what it was of is not known.
    #[test]
    fn an_unidentified_exercise_still_holds_its_sets() {
        let watch = account(5).expect("an account");
        let exercise = CanonicalExercise::ForReps {
            identified: Attributed::new(
                Identified::Guessed(GuessedExercise::Undetermined),
                watch,
            ),
            sets: NonEmpty::of(set(10, watch, None).expect("a set"), vec![]),
        };

        assert_eq!(exercise.exercise_key(), None);
        assert_eq!(exercise.set_count(), 1);
        assert!(!exercise.identified_by().as_i64().is_negative());
        assert_eq!(exercise.accounts(), vec![watch]);
    }
}
