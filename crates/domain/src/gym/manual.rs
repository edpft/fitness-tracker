//! A gym session the operator logged in a spreadsheet himself (#274).
//!
//! **A day, not an instant.** No sheet records when in the day he trained, and
//! some record only the week: `CT 2017` gives a week number and the day is the
//! one its programme put that workout on (operator, 2026-09-27). The day is
//! what the translation settles on, and where the sheet only implied it, it
//! still says so in the sheet the session names.
//!
//! **Its own set, because the load may be missing.** [`super::Load`] refuses a
//! case for a load nobody recorded, and rightly: a Hevy set always has one, and
//! a missing one there would merge bad data with an absent fact. A sheet is
//! different in kind. `the_beginner_prescription` computed every load from a
//! formula that came back empty, so its sets hold reps and reps in reserve and
//! no load at all, and the operator recorded the load on Garmin instead (#278
//! joins the two). So [`ManualSet::load`] is optional, and `None` means exactly
//! that the sheet does not say.
//!
//! **One session however many copies of its workbook hold it** (operator,
//! 2026-09-28). A workbook saved in several places is one record revised, not
//! several recordings, so its copies are merged before a session derives: every
//! detail any copy adds is kept, and where two copies state different values
//! for one fact the most recent workbook wins. A session therefore draws on
//! more than one file, and each set names the copy it was taken from.
//!
//! **Only what was performed.** A session derives here only where something
//! shows it was done: a note, an RPE, reps in reserve, a performed column, or a
//! sheet that is a log rather than a plan. A dated plan is a plan (operator,
//! 2026-09-27: *"if there isn't a positive sign that the session was
//! performed … we must assume that it was just a plan"*), and deciding that is
//! the adapter's job, because only the adapter can read the sheet.

use std::fmt;

use jiff::civil::Date;

use crate::{
    landing::{Cell, FilePath, LandingRecordId, SourceRecordId},
    measure::{Duration, Metres, RepCount},
    normalised::NormalisedEntity,
    sequence::NonEmpty,
};

use super::{
    exercise::{DistanceExercise, DurationExercise, RepsExercise},
    intensity::Rir,
    load::Load,
    outcome::Performed,
    set::SetKind,
};

/// One set, as a sheet recorded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualSet<M> {
    /// `None` where the sheet does not record the load, which is not the same
    /// as no load: an unloaded set is `Some` of [`Load::UNLOADED`].
    pub load: Option<Load>,
    pub outcome: Performed<M>,
    pub intensity: Option<Rir>,
    pub kind: SetKind,
    pub rest_after: Option<Duration>,
    /// The file, sheet and cell holding the set's count. A set names its own
    /// because a session need not sit on one sheet, or in one file: the 2018
    /// `1RM` sheets keep each lift on a sheet of its own, `CT 2017` keeps the
    /// trunk work apart from the lifts it was done with, and a session merged
    /// from copies of its workbook takes each set from the copy that says it.
    pub written_in: Cell,
}

/// One exercise and the sets performed of it, in order.
///
/// No supersets: no sheet records one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManualExercise {
    ForReps {
        exercise: RepsExercise,
        sets: NonEmpty<ManualSet<RepCount>>,
    },
    ForDuration {
        exercise: DurationExercise,
        sets: NonEmpty<ManualSet<Duration>>,
    },
    ForDistance {
        exercise: DistanceExercise,
        sets: NonEmpty<ManualSet<Metres>>,
    },
}

impl ManualExercise {
    pub const fn exercise_key(&self) -> &'static str {
        match self {
            Self::ForReps { exercise, .. } => exercise.as_str(),
            Self::ForDuration { exercise, .. } => exercise.as_str(),
            Self::ForDistance { exercise, .. } => exercise.as_str(),
        }
    }

    pub const fn measure(&self) -> &'static str {
        match self {
            Self::ForReps { .. } => "reps",
            Self::ForDuration { .. } => "duration",
            Self::ForDistance { .. } => "distance",
        }
    }

    pub const fn set_count(&self) -> usize {
        match self {
            Self::ForReps { sets, .. } => sets.count(),
            Self::ForDuration { sets, .. } => sets.count(),
            Self::ForDistance { sets, .. } => sets.count(),
        }
    }
}

/// The copy of a workbook a session is dated by: the most recent one that
/// holds it. Which file, sheet and cell each set came from, the set says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Logged {
    pub landed_as: LandingRecordId,
    /// The file's identity, which is the digest of its bytes.
    pub source_record_id: SourceRecordId,
    pub file: FilePath,
}

impl fmt::Display for Logged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file)
    }
}

/// A gym session the operator logged himself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManualGymSession {
    on: Date,
    logged: Logged,
    exercises: NonEmpty<ManualExercise>,
}

impl ManualGymSession {
    pub const fn new(on: Date, logged: Logged, exercises: NonEmpty<ManualExercise>) -> Self {
        Self {
            on,
            logged,
            exercises,
        }
    }

    pub const fn on(&self) -> Date {
        self.on
    }

    /// The most recent copy of its workbook that holds it.
    pub const fn logged(&self) -> &Logged {
        &self.logged
    }

    /// The exercises, in the order performed.
    pub const fn exercises(&self) -> &NonEmpty<ManualExercise> {
        &self.exercises
    }

    pub fn set_count(&self) -> usize {
        self.exercises
            .iter()
            .map(ManualExercise::set_count)
            .sum::<usize>()
    }
}

impl NormalisedEntity for ManualGymSession {
    /// Every copy it drew a set from, as well as the one it is dated by.
    fn composes(&self) -> Vec<&SourceRecordId> {
        let mut records = vec![&self.logged.source_record_id];
        for exercise in self.exercises.iter() {
            let cells: Vec<&Cell> = match exercise {
                ManualExercise::ForReps { sets, .. } => {
                    sets.iter().map(|set| &set.written_in).collect()
                }
                ManualExercise::ForDuration { sets, .. } => {
                    sets.iter().map(|set| &set.written_in).collect()
                }
                ManualExercise::ForDistance { sets, .. } => {
                    sets.iter().map(|set| &set.written_in).collect()
                }
            };
            for cell in cells {
                if !records.contains(&&cell.source_record_id) {
                    records.push(&cell.source_record_id);
                }
            }
        }
        records
    }
}

impl fmt::Display for ManualGymSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} — {} exercises, {} sets, {}",
            self.on,
            self.exercises.count(),
            self.set_count(),
            self.logged
        )
    }
}
