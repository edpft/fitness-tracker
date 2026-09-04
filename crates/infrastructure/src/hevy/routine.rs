//! Rendering an issued session as a Hevy routine.
//!
//! **A renderer, in the sense the terminal is one.** Nothing here decides what
//! to do in the gym; that was settled before this module was called. What it
//! decides is how an instruction survives the trip into a vocabulary that is
//! narrower than ours — and, where it cannot, that the loss is named rather than
//! quietly taken.
//!
//! ## What the source can hold, and what it cannot
//!
//! Verified against the published schema rather than remembered:
//!
//! - A rep **range** is native (`rep_range`), so `4-6` crosses as `4-6` rather
//!   than as a lie about four.
//! - A **warm-up** is a set type, so the ramp arrives marked as a ramp and does
//!   not inflate a volume count on the phone.
//! - **Supersets** are an id shared between exercises, and take as many members
//!   as we group.
//! - **Rest is per exercise, not per set.** Ours is per set, so the first
//!   instruction found is written and anything that disagrees goes to the notes.
//! - There is **no effort field on a routine set** — `rpe` exists when logging a
//!   workout and not when prescribing one. Every effort target is therefore a
//!   note, which is a change of medium rather than a loss.
//!
//! ## One exercise, one template, however many signs
//!
//! A Hevy exercise entry names exactly one template, and which template an
//! exercise is written to depends on the *sign* of the load
//! ([`super::writable`]). Sets of one exercise are therefore grouped by the
//! template they resolve to, and a run of sets that changes sign becomes two
//! entries rather than one entry with a wrong number in it. In practice every
//! session so far produces exactly one group per exercise; the grouping is what
//! makes the case where it does not a correct routine instead of a silent
//! coercion.

use std::fmt;

use application::{Deliverable, Unexpressed};
use domain::{
    gym::{Exercise, Kg, Load, Rir, Spans},
    prescription::{
        Prescribed, PrescribedExercise, PrescribedItem, PrescribedSet, SessionRole, Target,
        WeekKind,
    },
};
use serde::Serialize;
use serde_json::value::RawValue;

use super::writable::write_load;

/// The body of a create-routine request.
#[derive(Debug, Serialize)]
pub struct CreateRoutine {
    pub routine: RoutineBody,
}

#[derive(Debug, Serialize)]
pub struct RoutineBody {
    pub title: String,
    pub folder_id: Option<i64>,
    pub notes: String,
    pub exercises: Vec<RoutineExercise>,
}

#[derive(Debug, Serialize)]
pub struct RoutineExercise {
    pub exercise_template_id: String,
    /// Serialised even when absent. The published schema documents it as
    /// nullable with a null example, so an explicit null is what the source is
    /// described as expecting — omitting the key relies on a validator's
    /// tolerance instead.
    pub superset_id: Option<u32>,
    pub rest_seconds: Option<u64>,
    pub notes: String,
    pub sets: Vec<RoutineSet>,
}

#[derive(Debug, Serialize)]
pub struct RoutineSet {
    /// `normal` or `warmup`. The other two the source accepts — `dropset`,
    /// `failure` — describe how a set *went*, which is a performed fact and
    /// never something to prescribe.
    #[serde(rename = "type")]
    pub kind: &'static str,
    /// Serialised through [`RawValue`] rather than as an `f64`: a load is fixed
    /// point precisely so it survives a round trip, and turning 62.5 into a
    /// binary float on the way out would undo that at the last step.
    pub weight_kg: Option<Box<RawValue>>,
    pub reps: Option<u32>,
    pub rep_range: Option<RepRange>,
    pub distance_meters: Option<u64>,
    pub duration_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct RepRange {
    pub start: u32,
    pub end: u32,
}

/// A session, rendered — and whatever the source had no way to state.
pub struct Rendered {
    pub body: RoutineBody,
    pub unexpressed: Vec<Unexpressed>,
}

/// **Zero-padded, and the role after it.** The number orders the folder and the
/// role says what the session is; nothing else fits a phone's routine list at a
/// glance. Two digits because a block is a dozen sessions, and a wider one still
/// sorts correctly — it is the padding that makes 9 come before 10, not the
/// width.
fn title(session: &Deliverable) -> String {
    let role = match session.workout.session_role() {
        SessionRole::Light => "Light",
        SessionRole::Heavy => "Heavy",
    };
    format!("{:02} {role}", session.ordinal.as_u32())
}

/// What the routine says about itself, for an operator looking at it later.
///
/// The date it was issued for and where in the block it sits. Not the anchor and
/// not the parameters: those are recorded on the prescription, which is the
/// record, and repeating them here would put a second copy somewhere nothing
/// keeps current.
fn notes(session: &Deliverable) -> String {
    let week = match session.workout.week() {
        WeekKind::Climbing(index) => format!("week {}", index.as_u32()),
        WeekKind::Test => "test".to_owned(),
    };
    format!(
        "{} · {} · {}",
        session.workout.issued_for(),
        week,
        session.programme
    )
}

/// Render a session, and say what would not go.
pub fn render(session: &Deliverable, folder_id: Option<i64>) -> Rendered {
    let mut exercises = Vec::new();
    let mut unexpressed = Vec::new();
    let mut next_superset = 0_u32;

    for item in session.workout.shape().items().iter() {
        match item {
            PrescribedItem::Exercise { exercise, .. } => {
                render_exercise(exercise, None, &mut exercises, &mut unexpressed);
            }
            PrescribedItem::Superset(superset) => {
                let id = next_superset;
                next_superset = next_superset.saturating_add(1);
                for member in superset.members.iter() {
                    render_exercise(&member.exercise, Some(id), &mut exercises, &mut unexpressed);
                }
            }
        }
    }

    Rendered {
        body: RoutineBody {
            title: title(session),
            folder_id,
            notes: notes(session),
            exercises,
        },
        unexpressed,
    }
}

/// One prescribed exercise, as however many entries its loads require.
fn render_exercise(
    prescribed: &PrescribedExercise,
    superset_id: Option<u32>,
    into: &mut Vec<RoutineExercise>,
    unexpressed: &mut Vec<Unexpressed>,
) {
    let (exercise, rendered) = match prescribed {
        PrescribedExercise::ForReps { exercise, sets } => (
            Exercise::Reps(*exercise),
            sets.iter()
                .map(|set| reps_set(Exercise::Reps(*exercise), set))
                .collect::<Vec<_>>(),
        ),
        PrescribedExercise::ForDuration { exercise, sets } => (
            Exercise::Duration(*exercise),
            sets.iter()
                .map(|set| duration_set(Exercise::Duration(*exercise), set))
                .collect::<Vec<_>>(),
        ),
        PrescribedExercise::ForDistance { exercise, sets } => (
            Exercise::Distance(*exercise),
            sets.iter()
                .map(|set| distance_set(Exercise::Distance(*exercise), set))
                .collect::<Vec<_>>(),
        ),
    };

    let mut refused = 0_usize;
    let mut reason = String::new();
    let mut groups: Vec<(String, Vec<RoutineSet>)> = Vec::new();
    let mut annotations: Vec<String> = Vec::new();
    let mut rest_seconds = None;

    for outcome in rendered {
        match outcome {
            SetOutcome::Written {
                template_id,
                set,
                annotation,
                rest,
            } => {
                if let Some(annotation) = annotation
                    && !annotations.contains(&annotation)
                {
                    annotations.push(annotation);
                }
                // **The longest, not the first.** The primary's ramp rests
                // into its working set at the bottom of the range and between
                // working sets across the whole of it, so taking the first rest
                // found would put the warm-up's number on the exercise.
                rest_seconds = rest_seconds.max(rest);
                match groups.last_mut() {
                    // Consecutive sets on one template stay one entry; a change
                    // of sign opens a new one.
                    Some((current, sets)) if *current == template_id => sets.push(set),
                    _ => groups.push((template_id, vec![set])),
                }
            }
            SetOutcome::Refused { message } => {
                refused = refused.saturating_add(1);
                reason = message;
            }
        }
    }

    if refused > 0 {
        unexpressed.push(Unexpressed {
            exercise,
            reason: format!("{refused} of its sets could not be written: {reason}"),
        });
    }

    let notes = annotations.join("; ");
    for (template_id, sets) in groups {
        into.push(RoutineExercise {
            exercise_template_id: template_id,
            superset_id,
            rest_seconds,
            notes: notes.clone(),
            sets,
        });
    }
}

/// What became of one prescribed set.
enum SetOutcome {
    Written {
        template_id: String,
        set: RoutineSet,
        /// What the source has no field for, in words.
        annotation: Option<String>,
        rest: Option<u64>,
    },
    Refused {
        message: String,
    },
}

/// The load and the template, or why neither.
///
/// A set that pins no load still needs a template, so it resolves one at zero —
/// which is the same template any unloaded set of that exercise would take.
fn resolve(exercise: Exercise, load: Option<Load>) -> Result<(String, Option<Kg>), String> {
    let written = write_load(exercise, load.unwrap_or(Load::Absolute(Kg::NONE)))
        .map_err(|error| error.to_string())?;

    // A set that pins no load resolves a template and writes no weight, which is
    // how "work up to a single" reaches the phone as an empty weight field
    // rather than as a zero somebody might take literally.
    Ok((written.template_id.to_owned(), load.map(|_| written.weight)))
}

fn weight(kg: Option<Kg>) -> Option<Box<RawValue>> {
    kg.and_then(|kg| RawValue::from_string(kg.to_string()).ok())
}

const fn kind(warmup: bool) -> &'static str {
    if warmup { "warmup" } else { "normal" }
}

/// The rest a set instructs, at its longest.
///
/// **The top of the range, not the bottom.** The source takes one number per
/// exercise where we prescribe one per set, so something has to be chosen, and
/// the operator's instruction is to take the longest — a rest cut short is a
/// worse error than one overrun, and the range itself survives in the notes.
fn rest_of<M: Spans>(set: &PrescribedSet<M>) -> Option<u64> {
    set.rest_after.map(|rest| rest.maximum().as_seconds())
}

/// Everything about a set the source has no field for, as one phrase.
///
/// **This is the graceful-degradation path**, and it is deliberately the only
/// one: a routine set has no effort field, no per-set rest, and no range for a
/// hold or a carry — so each of those becomes words rather than being dropped.
/// An empty result means the source could state the set in full.
fn note_of<M: fmt::Display + Spans>(
    set: &PrescribedSet<M>,
    ranged: Option<String>,
) -> Option<String> {
    let parts: Vec<String> = [annotate(&set.prescription), ranged, rest_note(set)]
        .into_iter()
        .flatten()
        .collect();

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

/// The rest instruction as words, where it says more than one number can.
fn rest_note(set: &PrescribedSet<impl Spans>) -> Option<String> {
    match set.rest_after? {
        Target::Exactly(_) => None,
        range @ Target::Range { .. } => Some(format!("rest {range}")),
    }
}

/// The effort target, and anything else the source cannot state, in words.
fn annotate<M: std::fmt::Display + Spans>(prescription: &Prescribed<M>) -> Option<String> {
    match prescription {
        Prescribed::Fixed { effort, .. } => {
            effort.map(|effort| format!("{} in reserve", Rir::as_str(effort)))
        }
        Prescribed::ToEffort {
            effort, predicted, ..
        } => Some(predicted.as_ref().map_or_else(
            || format!("as many as, {} in reserve", Rir::as_str(*effort)),
            |measure| format!("~{measure}, {} in reserve", Rir::as_str(*effort)),
        )),
        Prescribed::Autoregulated {
            measure,
            effort,
            toward,
        } => Some(if toward.is_some() {
            format!(
                "work up to {measure}, {} in reserve — the weight shown is what the plan expects",
                Rir::as_str(*effort)
            )
        } else {
            format!(
                "work up to {measure}, {} in reserve — the load is the day's",
                Rir::as_str(*effort)
            )
        }),
    }
}

fn reps_set(exercise: Exercise, set: &PrescribedSet<domain::gym::RepCount>) -> SetOutcome {
    // **A destination that insists on a number gets the expected one.** Hevy has
    // no way to show "the load is the day's", so an autoregulated set with a
    // derived target is written at that target and the note says so.
    let planned = set
        .prescription
        .load()
        .or_else(|| set.prescription.toward());
    let (template_id, kg) = match resolve(exercise, planned) {
        Ok(resolved) => resolved,
        Err(message) => return SetOutcome::Refused { message },
    };

    let (reps, rep_range) = match set.prescription.measure() {
        Some(Target::Exactly(count)) => (Some(count.as_u32()), None),
        Some(range @ Target::Range { .. }) => (
            None,
            Some(RepRange {
                start: range.minimum().as_u32(),
                end: range.maximum().as_u32(),
            }),
        ),
        None => (None, None),
    };

    SetOutcome::Written {
        template_id,
        set: RoutineSet {
            kind: kind(set.warmup),
            weight_kg: weight(kg),
            reps,
            rep_range,
            distance_meters: None,
            duration_seconds: None,
        },
        annotation: note_of(set, None),
        rest: rest_of(set),
    }
}

fn duration_set(exercise: Exercise, set: &PrescribedSet<domain::gym::Duration>) -> SetOutcome {
    // **A destination that insists on a number gets the expected one.** Hevy has
    // no way to show "the load is the day's", so an autoregulated set with a
    // derived target is written at that target and the note says so.
    let planned = set
        .prescription
        .load()
        .or_else(|| set.prescription.toward());
    let (template_id, kg) = match resolve(exercise, planned) {
        Ok(resolved) => resolved,
        Err(message) => return SetOutcome::Refused { message },
    };

    // No range field for a hold, so the low bound is written and the range goes
    // to the notes — stated rather than rounded away.
    let (seconds, ranged) = match set.prescription.measure() {
        Some(Target::Exactly(duration)) => (Some(duration.as_seconds()), None),
        Some(range @ Target::Range { .. }) => {
            (Some(range.minimum().as_seconds()), Some(format!("{range}")))
        }
        None => (None, None),
    };

    SetOutcome::Written {
        template_id,
        set: RoutineSet {
            kind: kind(set.warmup),
            weight_kg: weight(kg),
            reps: None,
            rep_range: None,
            distance_meters: None,
            duration_seconds: seconds,
        },
        annotation: note_of(set, ranged),
        rest: rest_of(set),
    }
}

fn distance_set(exercise: Exercise, set: &PrescribedSet<domain::gym::Distance>) -> SetOutcome {
    // **A destination that insists on a number gets the expected one.** Hevy has
    // no way to show "the load is the day's", so an autoregulated set with a
    // derived target is written at that target and the note says so.
    let planned = set
        .prescription
        .load()
        .or_else(|| set.prescription.toward());
    let (template_id, kg) = match resolve(exercise, planned) {
        Ok(resolved) => resolved,
        Err(message) => return SetOutcome::Refused { message },
    };

    let metres = |distance: &domain::gym::Distance| distance.metres.as_millimetres() / 1_000;

    let (distance, ranged) = match set.prescription.measure() {
        Some(Target::Exactly(target)) => (Some(metres(target)), None),
        Some(range @ Target::Range { .. }) => {
            (Some(metres(&range.minimum())), Some(format!("{range}")))
        }
        None => (None, None),
    };

    SetOutcome::Written {
        template_id,
        set: RoutineSet {
            kind: kind(set.warmup),
            weight_kg: weight(kg),
            reps: None,
            rep_range: None,
            distance_meters: distance,
            duration_seconds: None,
        },
        annotation: note_of(set, ranged),
        rest: rest_of(set),
    }
}
