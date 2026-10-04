//! The operator's corrections to what a source said (§ II.2).
//!
//! An input, not a derivation. It is stored beside raw rather than inside the
//! normalised layer and consulted while that layer is rebuilt, so removing a
//! correction and re-deriving restores exactly what the source said. Nothing
//! here is ever written into a derived row.
//!
//! **Two fields so far, and § II.2 allows any of them.** A correction says
//! either which exercise a source named or what figures it recorded for one
//! set, and [`Corrected`] is which.
//!
//! **Which exercise a source named** is the one the record needed first: a
//! source with no entry for what the operator performed, logged under the
//! nearest thing it offered. The operator, 2026-09-29: *"Hevy didn't have a
//! neutral grip pull up, so I used pull up"*, and for two months `Stretching`
//! stood in for a squatting groin stretch. Nothing in the payload separates a
//! stand-in from the exercise it names, so no mapping table can tell them apart
//! — `Pull Up` is a pull-up on the days it means one.
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
//! **What figures it recorded** is a number entered wrongly rather than a
//! movement named wrongly, and it needed its own field (#351). 2018-06-11 holds a
//! deadlift set the watch states as 58 repetitions at 6 kg. The operator,
//! 2026-10-03: *"I think the 58 x 6kg is a transposition error, I think it's
//! actually 6x 57.5kg, entered incorrectly in the watch."* No merge rule
//! reaches it — the canonical layer is reporting the source faithfully and the
//! source is wrong — and § 6 allows the substitution because load and
//! repetitions are source-independent, so a corrected value is the same series
//! as the one it replaces. A method-dependent quantity would be excluded
//! instead of corrected.
//!
//! **Anchored to the source's own identity**, so a rebuild finds the same
//! observations: the record as the source names it, and the source's own word
//! for the thing inside it. Not the position of the entry within the record,
//! which moves when an entry is inserted or reordered, and not a surrogate key,
//! which a rebuild reissues. A term the source stops using is a correction that
//! stops applying, which is the honest outcome — the source is no longer saying
//! the thing that was wrong.
//!
//! **A figures correction carries what it replaces as well as what it
//! replaces it with**, and both have to match for it to apply. That is the same
//! rule one level down: a set whose figures have changed at source is no longer
//! the set the operator described, so the correction lapses rather than
//! overwriting a number nobody has ruled on.
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
use crate::measure::{Kg, RepCount};
use crate::newtype::string_name;
use crate::sequence::NonEmpty;

/// The source's own word for something inside one of its records.
///
/// Opaque here, and deliberately: for a movement, Hevy's is an
/// `exercise_template_id` and Garmin's is a classifier's term; for a set,
/// Garmin's is the instant it states the set began. Which it is belongs to the
/// adapter that reads it (§ 8). What the domain needs is that a correction can
/// name the thing the source named, not that it can interpret it.
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
/// One space across both kinds of correction, because `remove 4` has to name
/// one assertion and not two.
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

/// The figures one set carries: a count, and a mass where there was one.
///
/// **A mass and not a [`crate::gym::Load`].** Which axis a number is read on
/// belongs to the term it was typed against — Hevy's assisted pull-up template
/// records weight taken off — and correcting the number does not change the
/// term. That is the exercise correction's load-convention rule from the other
/// end: the correction supplies the number and the adapter still decides what
/// it means.
///
/// **No load is a real answer and not a missing one.** A set the operator typed
/// no weight against carries [`None`] here, and asserting a count with no mass
/// says the same thing about a set the source put a number on wrongly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetFigures {
    reps: RepCount,
    load: Option<Kg>,
}

impl SetFigures {
    pub const fn new(reps: RepCount, load: Option<Kg>) -> Self {
        Self { reps, load }
    }

    pub const fn reps(self) -> RepCount {
        self.reps
    }

    pub const fn load(self) -> Option<Kg> {
        self.load
    }
}

impl std::fmt::Display for SetFigures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.load {
            Some(load) => write!(f, "{} × {load} kg", self.reps),
            None => write!(f, "{} with no load recorded", self.reps),
        }
    }
}

/// What a correction replaces, and the observations it replaces it in.
///
/// The parts belong in one type because none of them is meaningful without the
/// others: an exercise with no entries corrects nothing, and a set of entries
/// with no exercise says nothing about them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Corrected {
    /// The movement the source named, over every entry it named it in.
    Exercise {
        exercise: Exercise,
        entries: NonEmpty<CorrectedTerm>,
    },
    /// What a set's figures read, over every set that recorded the figures it
    /// replaces.
    ///
    /// **`recorded` is the assertion's, not each set's.** One assertion names
    /// the sets of one day that recorded one pair of figures — "the set
    /// recorded as 58 × 6 kg" — so what it replaces is stated once and every
    /// set it reaches held it. That is also the guard: see the module's note on
    /// lapsing.
    Figures {
        recorded: SetFigures,
        figures: SetFigures,
        sets: NonEmpty<CorrectedTerm>,
    },
}

/// One assertion by the operator, and the observations it reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Correction {
    id: CorrectionId,
    asserted_at: Timestamp,
    reason: CorrectionReason,
    corrected: Corrected,
}

impl Correction {
    pub const fn new(
        id: CorrectionId,
        asserted_at: Timestamp,
        reason: CorrectionReason,
        corrected: Corrected,
    ) -> Self {
        Self {
            id,
            asserted_at,
            reason,
            corrected,
        }
    }

    pub const fn id(&self) -> CorrectionId {
        self.id
    }

    pub const fn asserted_at(&self) -> Timestamp {
        self.asserted_at
    }

    pub const fn reason(&self) -> &CorrectionReason {
        &self.reason
    }

    pub const fn corrected(&self) -> &Corrected {
        &self.corrected
    }

    /// How many observations this assertion reaches, for a report.
    #[must_use]
    pub const fn reaches(&self) -> usize {
        match &self.corrected {
            Corrected::Exercise { entries, .. } => entries.count(),
            Corrected::Figures { sets, .. } => sets.count(),
        }
    }
}

/// A figures correction as the overlay holds it.
///
/// Both figures, because applying it is conditional: the source has to still
/// record what the operator corrected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Replacement {
    recorded: SetFigures,
    reads: SetFigures,
}

/// Every correction in force for one stream, ready for a derivation to consult.
///
/// Built once per run and read per entry, so it is maps rather than lists: a
/// scan over 223 anchors for each of 700 records is a derivation getting slower
/// the more the operator corrects.
///
/// **Empty is the ordinary case and is not a special one.** A stream nobody has
/// corrected derives through the same path as one that has.
#[derive(Debug, Clone, Default)]
pub struct EditOverlay {
    exercises: HashMap<CorrectedTerm, Exercise>,
    figures: HashMap<CorrectedTerm, Replacement>,
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

        let mut exercises = HashMap::new();
        let mut figures = HashMap::new();
        for correction in ordered {
            match correction.corrected() {
                Corrected::Exercise {
                    exercise,
                    entries: terms,
                } => {
                    for term in terms {
                        exercises.insert(term.clone(), *exercise);
                    }
                }
                Corrected::Figures {
                    recorded,
                    figures: reads,
                    sets,
                } => {
                    for set in sets {
                        figures.insert(
                            set.clone(),
                            Replacement {
                                recorded: *recorded,
                                reads: *reads,
                            },
                        );
                    }
                }
            }
        }
        Self { exercises, figures }
    }

    /// What the operator says this entry actually was, if he has said.
    #[must_use]
    pub fn exercise_for(&self, record: &SourceRecordId, term: &str) -> Option<Exercise> {
        let Ok(term) = SourceTerm::try_from(term) else {
            return None;
        };
        self.exercises
            .get(&CorrectedTerm::new(record.clone(), term))
            .copied()
    }

    /// What the operator says this set's figures read, if he has said and the
    /// source still records the figures he corrected.
    ///
    /// `recorded` is what the source says now. A correction describing
    /// something else has lapsed: the set it was asserted over is not the set
    /// that is there, and § II.2's "overrides do not propagate" applies to a
    /// set that has changed as much as to a record that has just landed.
    #[must_use]
    pub fn figures_for(
        &self,
        record: &SourceRecordId,
        term: &str,
        recorded: SetFigures,
    ) -> Option<SetFigures> {
        let Ok(term) = SourceTerm::try_from(term) else {
            return None;
        };
        self.figures
            .get(&CorrectedTerm::new(record.clone(), term))
            .filter(|replacement| replacement.recorded == recorded)
            .map(|replacement| replacement.reads)
    }

    /// How many observations it reaches, for a derivation to report.
    #[must_use]
    pub fn count(&self) -> usize {
        self.exercises.len() + self.figures.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.exercises.is_empty() && self.figures.is_empty()
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

    fn figures(reps: u32, kilos: Option<u64>) -> SetFigures {
        SetFigures::new(
            RepCount::new(reps).expect("a positive count"),
            kilos.map(|kilos| Kg::from_grams(kilos * 1_000)),
        )
    }

    fn correction(
        id: i64,
        at_text: &str,
        exercise: Exercise,
        terms: Vec<CorrectedTerm>,
    ) -> Correction {
        Correction::new(
            CorrectionId::from(id),
            at(at_text),
            reason(),
            Corrected::Exercise {
                exercise,
                entries: NonEmpty::new(terms).expect("at least one term"),
            },
        )
    }

    fn figures_correction(
        id: i64,
        at_text: &str,
        recorded: SetFigures,
        reads: SetFigures,
        sets: Vec<CorrectedTerm>,
    ) -> Correction {
        Correction::new(
            CorrectionId::from(id),
            at(at_text),
            reason(),
            Corrected::Figures {
                recorded,
                figures: reads,
                sets: NonEmpty::new(sets).expect("at least one set"),
            },
        )
    }

    #[test]
    fn an_empty_overlay_corrects_nothing() {
        let overlay = EditOverlay::of(&[]);

        assert!(overlay.is_empty());
        assert_eq!(overlay.exercise_for(&record("a-workout"), "1B2B1E7C"), None);
        assert_eq!(
            overlay.figures_for(
                &record("an-activity"),
                "2018-06-11T06:25:22.0",
                figures(58, Some(6))
            ),
            None
        );
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
    fn a_figures_correction_replaces_the_set_it_was_asserted_over() {
        let overlay = EditOverlay::of(&[figures_correction(
            1,
            "2026-10-04T09:00:00Z",
            figures(58, Some(6)),
            figures(6, Some(57)),
            vec![CorrectedTerm::new(
                record("2768338072"),
                term("2018-06-11T06:25:22.0"),
            )],
        )]);

        assert_eq!(
            overlay.figures_for(
                &record("2768338072"),
                "2018-06-11T06:25:22.0",
                figures(58, Some(6))
            ),
            Some(figures(6, Some(57)))
        );
    }

    #[test]
    fn a_figures_correction_lapses_when_the_set_no_longer_records_them() {
        let overlay = EditOverlay::of(&[figures_correction(
            1,
            "2026-10-04T09:00:00Z",
            figures(58, Some(6)),
            figures(6, Some(57)),
            vec![CorrectedTerm::new(
                record("2768338072"),
                term("2018-06-11T06:25:22.0"),
            )],
        )]);

        // The source has since been edited: the set at that instant is a
        // different set from the one the operator ruled on.
        assert_eq!(
            overlay.figures_for(
                &record("2768338072"),
                "2018-06-11T06:25:22.0",
                figures(6, Some(50))
            ),
            None
        );
        // And another set in the same record is reached by neither.
        assert_eq!(
            overlay.figures_for(
                &record("2768338072"),
                "2018-06-11T06:31:04.0",
                figures(58, Some(6))
            ),
            None
        );
    }

    #[test]
    fn the_two_kinds_of_correction_do_not_reach_each_other() {
        let overlay = EditOverlay::of(&[
            correction(
                1,
                "2026-09-30T09:00:00Z",
                Exercise::Reps(RepsExercise::NeutralGripPullUp),
                vec![CorrectedTerm::new(record("shared"), term("a-term"))],
            ),
            figures_correction(
                2,
                "2026-10-04T09:00:00Z",
                figures(58, Some(6)),
                figures(6, Some(57)),
                vec![CorrectedTerm::new(record("shared"), term("a-term"))],
            ),
        ]);

        assert_eq!(
            overlay.exercise_for(&record("shared"), "a-term"),
            Some(Exercise::Reps(RepsExercise::NeutralGripPullUp))
        );
        assert_eq!(
            overlay.figures_for(&record("shared"), "a-term", figures(58, Some(6))),
            Some(figures(6, Some(57)))
        );
        assert_eq!(overlay.count(), 2);
    }

    #[test]
    fn a_reason_that_is_only_whitespace_is_not_a_reason() {
        assert!(CorrectionReason::try_from("   ").is_err());
        assert!(CorrectionReason::try_from(String::new()).is_err());
        assert!(SourceTerm::try_from(String::new()).is_err());
    }
}
