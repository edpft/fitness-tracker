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
//!   than as a lie about four. **The choice between a count and a range is the
//!   entry's, though, not the set's**, so where the two meet every count is
//!   widened to a degenerate range — see [`agree_on_one_schema`].
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
    gym::{Exercise, Load, Rir},
    measure::{Kg, Spans},
    prescription::{Prescribed, PrescribedExercise, PrescribedItem, PrescribedSet, Target},
    schedule::Relative,
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
    /// **Omitted rather than null**, unlike its siblings. The published schema
    /// calls it nullable on both endpoints and `POST` agrees, but `PUT` refuses
    /// a null — "Expected object, received null", once per set without a range.
    /// Found live on 2026-09-15, on the first replacement ever sent: a stub
    /// takes whatever it is given, so only the source could say.
    #[serde(skip_serializing_if = "Option::is_none")]
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
/// glance. Two digits because a macrocycle is a few dozen sessions, and a wider
/// one still sorts correctly — it is the padding that makes 9 come before 10,
/// not the width.
///
/// **The folder is the macrocycle, so the number is the macrocycle's** (#312).
/// It was the session's position in its *mesocycle* until 2026-09-30, which
/// restarted partway down a folder and put two `01`s in the autumn's.
///
/// **"Light" and "Heavy" are Hevy's words now, not the domain's.** A role is an
/// intensity and a volume since 2026-09-20 (issue #63) and prints as both; a
/// routine list on a phone has room for one word, and these are the two the
/// operator has been reading all year. Only the intensity is named because it
/// is what tells his two gym sessions apart.
fn title(session: &Deliverable) -> String {
    let role = match session.workout.session_role().intensity() {
        Relative::Lower => "Light",
        Relative::Higher => "Heavy",
    };
    format!("{:02} {role}", session.ordinal.as_u32())
}

/// What the routine says about itself, for an operator looking at it later.
///
/// **The session's address in the hierarchy, and nothing else** (#312): which
/// macrocycle, which mesocycle of it, which microcycle of that. The operator,
/// 2026-09-30, on what it should read: *"no date, just macrocycle, mesocycle,
/// microcycle"*. The date it was issued for went with the same change — Hevy
/// stamps a routine with the day it was created and the folder is one
/// macrocycle, so the note was spending a third of itself on the one fact the
/// phone already had.
///
/// Not the anchor and not the parameters: those are recorded on the
/// prescription, which is the record, and repeating them here would put a second
/// copy somewhere nothing keeps current.
fn notes(session: &Deliverable) -> String {
    // A holding week is no microcycle of this mesocycle, so it is named rather
    // than numbered (#190). Read from the resolved address and not from the
    // week's kind, so that what "no microcycle" means is decided in one place.
    let microcycle = session.microcycle.map_or_else(
        || "holding".to_owned(),
        |index| format!("microcycle {}", index.as_u32()),
    );
    // **A re-run says so, and only when it is one** (#313). The operator asked
    // for it because a second attempt at one microcycle otherwise reads as the
    // same routine twice, a week apart, with nothing to say why.
    let rerun = session
        .rerun
        .map_or_else(String::new, |rerun| format!(" (rerun {rerun})"));
    format!(
        "{} · mesocycle {} · {microcycle}{rerun}",
        session.plan, session.mesocycle
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
    for (template_id, mut sets) in groups {
        agree_on_one_schema(&mut sets);
        into.push(RoutineExercise {
            exercise_template_id: template_id,
            superset_id,
            rest_seconds,
            notes: notes.clone(),
            sets,
        });
    }
}

/// **One entry, one repetition schema** (#341).
///
/// Hevy's app holds the choice between a fixed count and a rep range on the
/// *exercise*, not on the set, and reads an entry as ranged the moment any one
/// of its sets carries a `rep_range`. Every fixed count in that entry then
/// renders blank. `05 Heavy`'s front squat reached the phone that way on
/// 2026-10-02: eight sets of `8, 8, 6, 5, 8` fixed and then `5-6` three times,
/// with nothing shown against the first five.
///
/// **Nothing was lost in transit** — the reply to the create had every one of
/// those counts back, so the loss is in a rendering we cannot change. A count
/// therefore shares an entry with a range by becoming one: `8` as `8-8`, which
/// the app accepts and shows as `8-8` (the operator, 2026-10-02). That is the
/// same instruction in the other notation rather than a degradation, which is
/// why it is not an [`Unexpressed`].
///
/// **A set that pins no measure is left alone.** There is no count to widen,
/// and a blank is the right rendering for a set that asks for none.
fn agree_on_one_schema(sets: &mut [RoutineSet]) {
    if !sets.iter().any(|set| set.rep_range.is_some()) {
        return;
    }

    for set in sets {
        if set.rep_range.is_some() {
            continue;
        }
        if let Some(reps) = set.reps.take() {
            set.rep_range = Some(RepRange {
                start: reps,
                end: reps,
            });
        }
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
        Prescribed::Autoregulated { measure, effort } => Some(format!(
            "work up to {measure}, {} in reserve — the load is the day's",
            Rir::as_str(*effort)
        )),
    }
}

fn reps_set(exercise: Exercise, set: &PrescribedSet<domain::measure::RepCount>) -> SetOutcome {
    let (template_id, kg) = match resolve(exercise, set.prescription.load()) {
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

fn duration_set(exercise: Exercise, set: &PrescribedSet<domain::measure::Duration>) -> SetOutcome {
    let (template_id, kg) = match resolve(exercise, set.prescription.load()) {
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

fn distance_set(exercise: Exercise, set: &PrescribedSet<domain::measure::Distance>) -> SetOutcome {
    let (template_id, kg) = match resolve(exercise, set.prescription.load()) {
        Ok(resolved) => resolved,
        Err(message) => return SetOutcome::Refused { message },
    };

    let metres = |distance: &domain::measure::Distance| distance.metres.as_millimetres() / 1_000;

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
