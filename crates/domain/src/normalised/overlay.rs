//! The operator's corrections to what a source said (§ II.2).
//!
//! An input, not a derivation. It is stored beside raw rather than inside the
//! normalised layer and consulted while that layer is rebuilt, so removing a
//! correction and re-deriving restores exactly what the source said. Nothing
//! here is ever written into a derived row.
//!
//! **What it corrects, so far, is which exercise a source named.** § II.2
//! allows a correction to any field of an observation, and this holds the one
//! the record needed: a source with no entry for what the operator performed,
//! logged under the nearest thing it offered. The operator, 2026-09-29: *"Hevy
//! didn't have a neutral grip pull up, so I used pull up"*, and for two months
//! `Stretching` stood in for a squatting groin stretch. Nothing in the payload
//! separates a stand-in from the exercise it names, so no mapping table can
//! tell them apart — `Pull Up` is a pull-up on the days it means one.
//!
//! **A correction says what the movement was, not how the source's numbers
//! read.** 139 of the 223 corrected pull-up sets were logged under Hevy's
//! assisted template, which records weight taken *off* and which the mapping
//! therefore negates; the other 84 were logged under templates recording weight
//! added. Carrying the corrected exercise's own load convention across would
//! turn 42kg of assistance into 42kg of added weight. The convention belongs to
//! the template the numbers were typed into, which the correction does not
//! change. § 8 makes this the same rule twice: assistance is a load axis of a
//! pull-up rather than a different exercise, so correcting the grip leaves the
//! axis alone.
//!
//! **Anchored to the source's own identity**, so a rebuild finds the same
//! observations: the record as the source names it, and the source's own word
//! for the movement. Not the position of the entry within the record, which
//! moves when an entry is inserted or reordered, and not a surrogate key, which
//! a rebuild reissues. A term the source stops using is a correction that stops
//! applying, which is the honest outcome — the source is no longer saying the
//! thing that was wrong.
//!
//! **Bulk is the action, not the record.** "Every pull-up since I left
//! CrossFit" names 223 sets over 69 sessions, and it is asserted once; what
//! is stored is one correction holding the observations that matched *when it
//! was made*. That is what keeps § II.2's last obligation — a record landed
//! afterwards does not inherit it, so a capture gap stays visible rather than
//! being papered over indefinitely.

use std::collections::HashMap;

use jiff::Timestamp;

use crate::gym::exercise::Exercise;
use crate::landing::{InvalidIdentifier, SourceRecordId};
use crate::newtype::string_name;
use crate::sequence::NonEmpty;

/// The source's own word for a movement.
///
/// Opaque here, and deliberately: Hevy's is an `exercise_template_id`, Garmin's
/// is a classifier's term, and which it is belongs to the adapter that reads it
/// (§ 8). What the domain needs is that a correction can name the thing the
/// source named, not that it can interpret it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceTerm(String);

impl TryFrom<String> for SourceTerm {
    type Error = InvalidIdentifier;

    fn try_from(term: String) -> Result<Self, Self::Error> {
        if term.is_empty() {
            return Err(InvalidIdentifier::Empty {
                field: "a source's term",
            });
        }
        Ok(Self(term))
    }
}

string_name!(SourceTerm, InvalidIdentifier);

/// Why the operator asserted a correction.
///
/// Required, because § II.2 says an unexplained override is unreadable six
/// months later. Non-empty is the whole of the validation: what counts as an
/// explanation is not ours to rule on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CorrectionReason(String);

impl TryFrom<String> for CorrectionReason {
    type Error = InvalidIdentifier;

    fn try_from(reason: String) -> Result<Self, Self::Error> {
        if reason.trim().is_empty() {
            return Err(InvalidIdentifier::Empty {
                field: "a correction's reason",
            });
        }
        Ok(Self(reason))
    }
}

string_name!(CorrectionReason, InvalidIdentifier);

/// What the store calls one correction.
///
/// A surrogate, and allowed to be: it identifies the *assertion* so the
/// operator can retract it, and it is not what any observation is anchored by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CorrectionId(i64);

impl From<i64> for CorrectionId {
    fn from(id: i64) -> Self {
        Self(id)
    }
}

impl CorrectionId {
    pub const fn as_i64(self) -> i64 {
        self.0
    }
}

impl std::fmt::Display for CorrectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One observation a correction names, in the source's own terms.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CorrectedTerm {
    record: SourceRecordId,
    term: SourceTerm,
}

impl CorrectedTerm {
    pub const fn new(record: SourceRecordId, term: SourceTerm) -> Self {
        Self { record, term }
    }

    pub const fn record(&self) -> &SourceRecordId {
        &self.record
    }

    pub const fn term(&self) -> &SourceTerm {
        &self.term
    }
}

/// One assertion by the operator, and the observations it reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    id: CorrectionId,
    exercise: Exercise,
    asserted_at: Timestamp,
    reason: CorrectionReason,
    terms: NonEmpty<CorrectedTerm>,
}

impl Correction {
    pub const fn new(
        id: CorrectionId,
        exercise: Exercise,
        asserted_at: Timestamp,
        reason: CorrectionReason,
        terms: NonEmpty<CorrectedTerm>,
    ) -> Self {
        Self {
            id,
            exercise,
            asserted_at,
            reason,
            terms,
        }
    }

    pub const fn id(&self) -> CorrectionId {
        self.id
    }

    pub const fn exercise(&self) -> Exercise {
        self.exercise
    }

    pub const fn asserted_at(&self) -> Timestamp {
        self.asserted_at
    }

    pub const fn reason(&self) -> &CorrectionReason {
        &self.reason
    }

    pub const fn terms(&self) -> &NonEmpty<CorrectedTerm> {
        &self.terms
    }
}

/// Every correction in force for one stream, ready for a derivation to consult.
///
/// Built once per run and read per entry, so it is a map rather than a list: a
/// scan over 223 anchors for each of 700 records is a derivation getting slower
/// the more the operator corrects.
///
/// **Empty is the ordinary case and is not a special one.** A stream nobody has
/// corrected derives through the same path as one that has.
#[derive(Debug, Clone, Default)]
pub struct EditOverlay {
    corrected: HashMap<CorrectedTerm, Exercise>,
}

impl EditOverlay {
    /// The overlay those corrections make.
    ///
    /// **Later wins where two corrections name one observation.** The operator
    /// asserting something new about a set he has already corrected is him
    /// changing his mind, and the alternative — refusing the second assertion —
    /// would make him retract the first before he could. Ordering is by when
    /// they were asserted, so the result does not depend on the order the store
    /// hands them back in.
    #[must_use]
    pub fn of(corrections: &[Correction]) -> Self {
        let mut ordered: Vec<&Correction> = corrections.iter().collect();
        ordered.sort_by_key(|correction| (correction.asserted_at(), correction.id()));

        let mut corrected = HashMap::new();
        for correction in ordered {
            for term in correction.terms() {
                corrected.insert(term.clone(), correction.exercise());
            }
        }
        Self { corrected }
    }

    /// What the operator says this entry actually was, if he has said.
    #[must_use]
    pub fn exercise_for(&self, record: &SourceRecordId, term: &str) -> Option<Exercise> {
        let Ok(term) = SourceTerm::try_from(term) else {
            return None;
        };
        self.corrected
            .get(&CorrectedTerm::new(record.clone(), term))
            .copied()
    }

    /// How many observations it reaches, for a derivation to report.
    #[must_use]
    pub fn count(&self) -> usize {
        self.corrected.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.corrected.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gym::exercise::{DurationExercise, RepsExercise};

    fn record(id: &str) -> SourceRecordId {
        SourceRecordId::try_from(id).expect("a non-empty source record id")
    }

    fn term(term: &str) -> SourceTerm {
        SourceTerm::try_from(term).expect("a non-empty term")
    }

    fn reason() -> CorrectionReason {
        CorrectionReason::try_from("Hevy had no neutral grip pull up").expect("a non-empty reason")
    }

    fn at(rfc3339: &str) -> Timestamp {
        rfc3339.parse().expect("an RFC 3339 instant")
    }

    fn correction(
        id: i64,
        at_text: &str,
        exercise: Exercise,
        terms: Vec<CorrectedTerm>,
    ) -> Correction {
        Correction::new(
            CorrectionId::from(id),
            exercise,
            at(at_text),
            reason(),
            NonEmpty::new(terms).expect("at least one term"),
        )
    }

    #[test]
    fn an_empty_overlay_corrects_nothing() {
        let overlay = EditOverlay::of(&[]);

        assert!(overlay.is_empty());
        assert_eq!(overlay.exercise_for(&record("a-workout"), "1B2B1E7C"), None);
    }

    #[test]
    fn a_correction_reaches_the_term_it_names_and_no_other() {
        let overlay = EditOverlay::of(&[correction(
            1,
            "2026-09-30T09:00:00Z",
            Exercise::Reps(RepsExercise::NeutralGripPullUp),
            vec![CorrectedTerm::new(record("a-workout"), term("1B2B1E7C"))],
        )]);

        assert_eq!(
            overlay.exercise_for(&record("a-workout"), "1B2B1E7C"),
            Some(Exercise::Reps(RepsExercise::NeutralGripPullUp))
        );
        // The same term in a different record, and a different term in the same
        // one. Anchoring on the pair is what keeps both untouched.
        assert_eq!(
            overlay.exercise_for(&record("another-workout"), "1B2B1E7C"),
            None
        );
        assert_eq!(overlay.exercise_for(&record("a-workout"), "2C37EC5E"), None);
    }

    #[test]
    fn the_later_assertion_wins_and_the_store_order_does_not_matter() {
        let earlier = correction(
            1,
            "2026-09-01T09:00:00Z",
            Exercise::Duration(DurationExercise::Stretching),
            vec![CorrectedTerm::new(record("a-workout"), term("527DA061"))],
        );
        let later = correction(
            2,
            "2026-09-30T09:00:00Z",
            Exercise::Duration(DurationExercise::SquattingGroinStretch),
            vec![CorrectedTerm::new(record("a-workout"), term("527DA061"))],
        );

        let forwards = EditOverlay::of(&[earlier.clone(), later.clone()]);
        let backwards = EditOverlay::of(&[later, earlier]);

        let expected = Some(Exercise::Duration(DurationExercise::SquattingGroinStretch));
        assert_eq!(
            forwards.exercise_for(&record("a-workout"), "527DA061"),
            expected
        );
        assert_eq!(
            backwards.exercise_for(&record("a-workout"), "527DA061"),
            expected
        );
    }

    #[test]
    fn a_reason_that_is_only_whitespace_is_not_a_reason() {
        assert!(CorrectionReason::try_from("   ").is_err());
        assert!(CorrectionReason::try_from(String::new()).is_err());
        assert!(SourceTerm::try_from(String::new()).is_err());
    }
}
