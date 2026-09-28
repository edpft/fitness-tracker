//! The gym session that happened, however many sources recorded it.
//!
//! The canonical layer's entity (§ II.4), and the operator's ask, 2026-09-28:
//! *"a canonical view of performed workouts that provides the fullest view
//! possible by joining Garmin (Heart Rate and Exercise), Hevy, BTWB, and
//! historical spreadsheet data."*
//!
//! **It names its parts and holds none of them.** The session says what
//! happened and which normalised session each part of it comes from; the
//! exercises, the sets and the heart rate stay where they were derived.
//!
//! That is not a smaller version of a session that holds its own sets. It is
//! what the record turns out to require. On 2019-03-14 three normalised
//! sessions describe one visit, and no two of them agree:
//!
//! | | `1RM.xlsx` | `Strength training 2019.xlsx` | Garmin |
//! |---|---|---|---|
//! | bench | 47.5×5, 52.5×5, 57.5×3, 62.5×3, 65×1, 67.5×1, 70×1, 60×5×3 | the same | 48×5, 53×5, 58×3, 63×3, 65×1, 68×1, 70×1, 60×5×3 |
//! | Romanian deadlift | 70×10 ×3 | 70×10 ×3 | 70×10 ×3, called a deadlift |
//! | face pull | 27, 27, 27 | 32, 32, 27 | 32, 32, 27 |
//! | front squat | 40, 40, 40×10 | 45, 45, 45×12 | 45, 45, 45×12 |
//! | clock | the day | the day | 07:43–08:28, per set |
//! | heart rate | — | — | 77 average, 133 highest |
//!
//! Garmin settles which sheet was the template and which recorded the session,
//! and is itself wrong twice: it states 48 kg where both sheets say 47.5,
//! because the load is entered on a dial that takes whole kilogrammes and a
//! barbell lift is almost always a multiple of 2.5 (operator, 2026-09-28); and
//! it calls the Romanian deadlifts deadlifts, because that is its classifier's
//! guess ([`super::GuessedExercise`]).
//!
//! **So each source is authoritative about different things, and no one of
//! them is authoritative about a set.** The sheet has the load the watch
//! cannot express. The watch has the clock nothing else recorded. A merged
//! sequence of sets would have to choose between 47.5 and 48 and throw one
//! away, which § 10 puts at the analytical layer and not here: *"Records from
//! different sources are co-observations. Neither supersedes the other, both
//! stand, and disagreement between them is evidence rather than error."*
//!
//! It is not only the old record. Twelve days hold both a Hevy session and a
//! Beyond The White Board one, and on every one of them Hevy has warm-ups BTWB
//! never sees while BTWB has exercises Hevy never logged. The two also name
//! the same movement differently, and there the gym's log is the one to
//! believe: BTWB carries the prescription, so its `thruster-dumbbell` is what
//! was programmed, while Hevy's `thruster-kettlebell` is the closest entry in
//! Hevy's catalogue at the time (operator, 2026-09-28).
//!
//! **So the join is by part, not by set.** Of the operator's 550 gym days, 360
//! have one source, 180 have two and 10 have three; the ordinary modern shape
//! is Hevy's exercises with a watch's heart rate, on 122 of them.
//!
//! **What this does not hold.** No duration and no set count: both are
//! functions of the parts, and § II.4 keeps derived figures at the analytical
//! layer. No prescription either — § 11 stores prescribed and performed
//! separately, and what a workout was performed against is the normalised
//! workout's.

use std::fmt;

use crate::canonical::{NormalisedSessionId, Occurred};
use crate::sequence::NonEmpty;

/// One part of a session, and the normalised session it comes from.
///
/// **Two parts because that is what the sources split on.** A watch records
/// a session's heart rate and, separately, what it made of the exercises
/// (`MeasuredGymSession`); every other source records exercises and no heart
/// rate at all. The split was the operator's instruction, 2026-09-18, and this
/// is the other end of it.
///
/// **A normalised session may supply both, and may supply neither.** Garmin's
/// 2019-03-14 activity is two parts of the one canonical session and appears
/// twice here; its 2025-02-19 activity supplies the heart rate alone, because
/// the single unloaded set the watch classified on its own is not an account
/// of the exercises Hevy holds. Which is which is the matching's to decide
/// (#247) — this type only records what it decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Part {
    /// The exercises performed and the sets performed of them.
    Exercises(NormalisedSessionId),
    /// What the heart did while they were performed.
    HeartRate(NormalisedSessionId),
}

impl Part {
    /// The normalised session this part comes from.
    pub const fn from(self) -> NormalisedSessionId {
        match self {
            Self::Exercises(session) | Self::HeartRate(session) => session,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exercises(_) => "exercises",
            Self::HeartRate(_) => "heart rate",
        }
    }
}

impl fmt::Display for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} from {}", self.as_str(), self.from())
    }
}

/// One gym session that happened, however many sources recorded it.
///
/// Non-empty parts as a type rather than as a promise: a canonical session
/// standing on nothing asserts that a session happened while naming no
/// observation of it, which is the one thing this layer cannot do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalGymSession {
    occurred: Occurred,
    parts: NonEmpty<Part>,
}

impl CanonicalGymSession {
    pub const fn new(occurred: Occurred, parts: NonEmpty<Part>) -> Self {
        Self { occurred, parts }
    }

    /// When it happened, at the precision its sources knew it to.
    pub const fn occurred(&self) -> &Occurred {
        &self.occurred
    }

    pub const fn parts(&self) -> &NonEmpty<Part> {
        &self.parts
    }

    /// The normalised sessions its exercises come from, in the order matched.
    ///
    /// More than one where two sources gave different accounts of them, which
    /// is 2019-03-14 and the twelve Hevy-and-BTWB days. None where nothing
    /// recorded any, which is the 307 Garmin activities that are a heart rate
    /// and nothing else.
    pub fn exercises_from(&self) -> impl Iterator<Item = NormalisedSessionId> + '_ {
        self.parts.iter().filter_map(|part| match part {
            Part::Exercises(session) => Some(*session),
            Part::HeartRate(_) => None,
        })
    }

    /// The normalised sessions its heart rate comes from.
    ///
    /// Only a watch records one, so at most one today. A plural return rather
    /// than an `Option` because nothing in the model says a second device
    /// cannot, and § 10 would keep both if one did.
    pub fn heart_rate_from(&self) -> impl Iterator<Item = NormalisedSessionId> + '_ {
        self.parts.iter().filter_map(|part| match part {
            Part::HeartRate(session) => Some(*session),
            Part::Exercises(_) => None,
        })
    }

    /// Every normalised session it stands on, each once, in the order matched.
    ///
    /// § II.4: provenance survives reconciliation, and a canonical entity
    /// always names the normalised entities it stands for. This is that list.
    pub fn stands_on(&self) -> Vec<NormalisedSessionId> {
        let mut seen = Vec::with_capacity(self.parts.count());
        for part in self.parts.iter() {
            let session = part.from();
            if !seen.contains(&session) {
                seen.push(session);
            }
        }
        seen
    }
}

impl fmt::Display for CanonicalGymSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — exercises from {}, heart rate from {}",
            self.occurred,
            self.exercises_from().count(),
            self.heart_rate_from().count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalised::{OperatorZone, StartedAt, UnknownTimeZone};
    use jiff::Timestamp;

    fn zone() -> Result<OperatorZone, UnknownTimeZone> {
        OperatorZone::try_from("Europe/London".to_owned())
    }

    fn id(
        value: i64,
    ) -> Result<NormalisedSessionId, crate::canonical::NegativeNormalisedSessionId> {
        NormalisedSessionId::try_from(value)
    }

    /// 2019-03-14, as three normalised sessions describe it: two accounts of
    /// the exercises that do not agree, and one heart rate.
    #[test]
    fn a_session_may_stand_on_two_accounts_of_its_exercises() {
        let sheets = (
            Part::Exercises(id(1).expect("an id")),
            Part::Exercises(id(2).expect("an id")),
        );
        let watch = Part::HeartRate(id(3).expect("an id"));
        let session = CanonicalGymSession::new(
            Occurred::On("2019-03-14".parse().expect("a day")),
            NonEmpty::of(sheets.0, vec![sheets.1, watch]),
        );

        assert_eq!(
            session.exercises_from().collect::<Vec<_>>(),
            vec![id(1).expect("an id"), id(2).expect("an id")]
        );
        assert_eq!(
            session.heart_rate_from().collect::<Vec<_>>(),
            vec![id(3).expect("an id")]
        );
    }

    /// A watch that supplies both parts is named once, not twice: § II.4 wants
    /// the entities it stands for, and it stands on one.
    #[test]
    fn a_session_names_each_normalised_session_once() {
        let watch = id(7).expect("an id");
        let session = CanonicalGymSession::new(
            Occurred::On("2019-03-14".parse().expect("a day")),
            NonEmpty::of(Part::Exercises(watch), vec![Part::HeartRate(watch)]),
        );

        assert_eq!(session.stands_on(), vec![watch]);
        assert_eq!(session.parts().count(), 2);
    }

    /// 307 of the operator's Garmin activities are a heart rate and nothing
    /// else, and that is a session rather than a session with a hole in it.
    #[test]
    fn a_session_may_have_no_account_of_its_exercises() {
        let session = CanonicalGymSession::new(
            Occurred::On("2015-02-09".parse().expect("a day")),
            NonEmpty::of(Part::HeartRate(id(1).expect("an id")), vec![]),
        );

        assert_eq!(session.exercises_from().count(), 0);
        assert_eq!(session.heart_rate_from().count(), 1);
    }

    /// The day is the operator's, resolved through the zone. 00:30 on 14 March
    /// in Europe/London is 00:30 UTC in winter, but the same wall clock in
    /// August is the 13th in UTC — and the session is still the 14th's.
    #[test]
    fn the_day_is_the_local_one() {
        let instant: Timestamp = "2019-08-13T23:30:00Z".parse().expect("an instant");
        let session = CanonicalGymSession::new(
            Occurred::At(StartedAt::new(instant, zone().expect("a zone"))),
            NonEmpty::of(Part::HeartRate(id(1).expect("an id")), vec![]),
        );

        assert_eq!(
            session.occurred().day(),
            "2019-08-14".parse().expect("a day")
        );
    }

    #[test]
    fn a_dated_session_states_no_instant() {
        let occurred = Occurred::On("2019-03-14".parse().expect("a day"));

        assert!(occurred.instant().is_none());
        assert_eq!(occurred.day(), "2019-03-14".parse().expect("a day"));
    }

    #[test]
    fn a_part_reads_as_what_it_is() {
        let part = Part::HeartRate(id(3).expect("an id"));

        assert_eq!(part.as_str(), "heart rate");
        assert_eq!(part.to_string(), "heart rate from 3");
    }
}
