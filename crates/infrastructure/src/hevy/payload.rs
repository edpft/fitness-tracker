//! The shape of a Hevy workout payload, and nothing more.
//!
//! This is the only place in the build that knows what Hevy's JSON looks like.
//! It reads; it does not interpret. Deciding what a field *means* — which
//! exercise a template is, whether a weight is absolute or a delta — is
//! [`super::mapping`]'s job, and building the entity is
//! [`super::translate`]'s.
//!
//! **Numbers arrive as their own bytes.** `weight_kg`, `distance_meters` and
//! `rpe` are `&RawValue`, so the translator sees the characters the source
//! wrote — `77.5`, `20.4`, `9.5` — and parses them into fixed point directly.
//! Deserialising them as `f64` would lose before anything else got a chance to
//! be careful: by then `20.4` is `20.399999999999998578…`, and a load is
//! persisted, digested and compared against rows written by earlier versions.
//!
//! Fields we do not model are simply absent from these structs — the title, the
//! description, the notes, the `routine_id`. Serde ignores what it is not asked
//! for, raw retains all of it, and a later feature can add a field here without
//! anything having been lost in between.

use serde::Deserialize;
use serde_json::value::RawValue;

/// Why a payload could not be read as a workout.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{detail}")]
pub struct UnreadablePayload {
    pub detail: String,
}

/// One workout, as the events feed serves it.
///
/// The envelope's `type` is not read here. The adapter already recorded it as
/// provenance when the record landed, and that is the authoritative copy —
/// reading it again from the body would be a second answer to a question that
/// already has one.
#[derive(Debug, Deserialize)]
pub struct WorkoutEnvelope<'a> {
    #[serde(borrow)]
    pub workout: Option<Workout<'a>>,
}

#[derive(Debug, Deserialize)]
pub struct Workout<'a> {
    /// A true UTC instant, not a naive wall clock stamped `Z`: starts cluster
    /// at 18:00 UTC through British Summer Time and 19:00–20:00 through
    /// Greenwich Mean Time, a clean one-hour shift.
    pub start_time: String,
    #[serde(borrow, default)]
    pub exercises: Vec<ExerciseEntry<'a>>,
}

#[derive(Debug, Deserialize)]
pub struct ExerciseEntry<'a> {
    /// Positional, and the only identity below the workout that Hevy publishes.
    /// It moves under insertion or reordering, which is why an overlay anchored
    /// here is an open question — but for naming the place a refusal happened
    /// it is exactly what an operator sees in the app.
    pub index: u32,
    pub exercise_template_id: String,
    /// Not stable, and never used to key anything. `Overhead Squat` has two
    /// template ids and template `DDB29047` has appeared under two titles.
    /// Carried only so a refusal can be read without opening the payload.
    #[serde(default)]
    pub title: String,
    /// Which grouping this entry belongs to, if any. Small integers, unique
    /// only within a workout.
    #[serde(default)]
    pub superset_id: Option<u32>,
    #[serde(borrow, default)]
    pub sets: Vec<PerformedSet<'a>>,
}

#[derive(Debug, Deserialize)]
pub struct PerformedSet<'a> {
    pub index: u32,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(borrow, default)]
    pub weight_kg: Option<&'a RawValue>,
    #[serde(default)]
    pub reps: Option<u32>,
    #[serde(borrow, default)]
    pub distance_meters: Option<&'a RawValue>,
    #[serde(default)]
    pub duration_seconds: Option<u64>,
    /// Recorded as RPE and glossed as reps in reserve in Hevy's own interface.
    /// Eight positions: `6, 7, 7.5, 8, 8.5, 9, 9.5, 10`.
    #[serde(borrow, default)]
    pub rpe: Option<&'a RawValue>,
}

impl<'a> WorkoutEnvelope<'a> {
    /// # Errors
    ///
    /// [`UnreadablePayload`] if the bytes are not a workout event.
    pub fn read(bytes: &'a [u8]) -> Result<Self, UnreadablePayload> {
        serde_json::from_slice(bytes).map_err(|error| UnreadablePayload {
            detail: error.to_string(),
        })
    }
}

/// The characters a number was written with, or `None` where the field was
/// absent or null.
///
/// The whole reason [`RawValue`] is used: this hands the translator a decimal
/// string to parse exactly, never a float to round.
pub fn number(raw: Option<&RawValue>) -> Option<&str> {
    raw.map(RawValue::get).filter(|token| *token != "null")
}
