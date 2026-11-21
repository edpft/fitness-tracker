//! Which of Hevy's entries a correction would reach.
//!
//! The operator names the correction in *our* vocabulary — "every pull-up since
//! I left CrossFit" — and what has to be stored is the source's own identity
//! for each entry that answers to it. Resolving one into the other needs Hevy's
//! payloads and Hevy's mapping, so it belongs here rather than in a use case:
//! `pull-up` reaches three templates (`Pull Up`, `Pull Up (Assisted)` and
//! `Pull Up (Band)`), and which three is a fact about the source.
//!
//! **Enumerated from the payloads rather than from the normalised layer.** The
//! normalised layer holds our exercise and has already forgotten the template
//! that produced it, and the template is what the correction anchors on.
//!
//! **Enumerated at assertion time, which is the point.** § II.2 says an
//! override does not propagate, so what is stored is the entries that matched
//! when the operator asserted it. A workout landed next week does not inherit
//! it, and that keeps a capture gap visible.
//!
//! **A withdrawn record is not a candidate.** A source record with a deleted
//! event has no normalised entity at all (§ II.3), so correcting an entry in
//! one would be an assertion about nothing.

use application::StoreError;
use domain::{
    gym::{exercise::Exercise, intensity::Rir},
    landing::SourceRecordId,
    normalised::{CorrectedTerm, EditOverlay, SetEdit, SetFigures, SourceTerm, StatedZones},
};
use jiff::{Timestamp, civil::Date};
use sqlx::SqlitePool;
use std::collections::{BTreeMap, BTreeSet};

use crate::store::store_error;

use super::{
    mapping::lookup,
    payload::WorkoutEnvelope,
    translate::{intensity_of, recorded_figures, set_term},
};
use crate::garmin::FoundSet;

/// One entry a correction would reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandIn {
    /// What the correction is anchored by.
    pub term: CorrectedTerm,
    /// The day it was performed, in the operator's zone, for the report.
    pub day: Date,
    /// Hevy's own title for the template, so the operator can recognise it.
    pub title: String,
    /// How many sets it carries, which is the figure that makes the scale of an
    /// assertion legible.
    pub sets: usize,
}

/// One set as Hevy logged it, and what the operator has changed about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedSet {
    /// What a correction of it is anchored by.
    pub term: CorrectedTerm,
    /// Its count and mass as logged; [`None`] for a failed attempt, which has
    /// no count to anchor a correction on.
    pub recorded: Option<SetFigures>,
    pub warm_up: bool,
    /// Its effort as logged.
    pub effort: Option<Rir>,
    /// What the operator has changed about it, where he has.
    pub edit: Option<SetEdit>,
}

/// The days a search for entries reaches, both inclusive and either open.
#[derive(Debug, Clone, Copy, Default)]
pub struct Within {
    pub from: Option<Date>,
    pub until: Option<Date>,
}

/// Hevy's landed workouts, asked which entries currently read as an exercise.
#[derive(Debug, Clone)]
pub struct HevyStandIns {
    pool: SqlitePool,
}

impl HevyStandIns {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The effective serving of each record, the way a derivation sees it: a
    /// later serving replaces an earlier one, and a deletion *anywhere* in a
    /// record's history withdraws it whatever order the events arrived in.
    /// Absorbing rather than latest-wins, because that is what
    /// [`super::translate`] does and an enumeration that disagreed with the
    /// derivation would offer the operator entries it will never correct.
    async fn current(&self) -> Result<BTreeMap<String, Vec<u8>>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT source_record_id AS "source_record_id!: String",
                   event_kind       AS "event_kind!: String",
                   payload          AS "payload!: Vec<u8>"
            FROM hevy_workout_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut withdrawn: BTreeSet<String> = BTreeSet::new();
        let mut latest: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for row in rows {
            if row.event_kind == "deleted" {
                withdrawn.insert(row.source_record_id);
                continue;
            }
            // Ordered by id, so the last write for a record is its latest
            // serving.
            latest.insert(row.source_record_id, row.payload);
        }
        latest.retain(|record, _| !withdrawn.contains(record));
        Ok(latest)
    }

    /// Every set logged on `day` under an entry that reads as `exercise`, in
    /// the order performed (#892): workouts by their start, then Hevy's own
    /// order within each.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read or holds a payload that is
    /// not a workout.
    pub async fn sets_of(
        &self,
        exercise: Exercise,
        zones: &StatedZones,
        overlay: &EditOverlay,
        day: Date,
    ) -> Result<Vec<LoggedSet>, StoreError> {
        let mut workouts = Vec::new();
        for (source_record_id, payload) in self.current().await? {
            let envelope =
                WorkoutEnvelope::read(&payload).map_err(|error| StoreError::Corrupt {
                    detail: format!("{source_record_id}: {}", error.detail),
                })?;
            let Some(workout) = envelope.workout else {
                continue;
            };
            let Ok(instant) = workout.start_time.parse::<Timestamp>() else {
                continue;
            };
            if zones.started_at(instant).wall_clock().date() != day {
                continue;
            }
            let record = SourceRecordId::try_from(source_record_id.clone()).map_err(|error| {
                StoreError::Corrupt {
                    detail: error.to_string(),
                }
            })?;
            let mut sets = Vec::new();
            for entry in &workout.exercises {
                let reads = overlay
                    .exercise_for(&record, &entry.exercise_template_id)
                    .or_else(|| lookup(&entry.exercise_template_id).map(|mapped| mapped.exercise));
                if reads != Some(exercise) {
                    continue;
                }
                for set in &entry.sets {
                    let Ok(term) =
                        SourceTerm::try_from(set_term(&entry.exercise_template_id, set.index))
                    else {
                        continue;
                    };
                    let recorded = recorded_figures(set);
                    sets.push(LoggedSet {
                        edit: recorded
                            .and_then(|recorded| overlay.set_for(&record, term.as_str(), recorded)),
                        term: CorrectedTerm::new(record.clone(), term),
                        recorded,
                        warm_up: set.kind == "warmup",
                        effort: intensity_of(set).ok().flatten(),
                    });
                }
            }
            workouts.push((instant, sets));
        }
        workouts.sort_by_key(|(instant, _)| *instant);
        Ok(workouts.into_iter().flat_map(|(_, sets)| sets).collect())
    }

    /// Every entry that currently translates to `exercise`, within the days
    /// given, in the order it was performed.
    ///
    /// `from` and `until` are inclusive and either may be absent, because "every
    /// pull-up since I left CrossFit" has a start and no end while "everything
    /// before Hevy had the movement" has an end and no start.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read or holds a payload that is
    /// not a workout.
    pub async fn reading_as(
        &self,
        exercise: Exercise,
        zones: &StatedZones,
        overlay: &EditOverlay,
        within: &Within,
    ) -> Result<Vec<StandIn>, StoreError> {
        let latest = self.current().await?;

        let mut found = Vec::new();
        for (source_record_id, payload) in latest {
            let envelope =
                WorkoutEnvelope::read(&payload).map_err(|error| StoreError::Corrupt {
                    detail: format!("{source_record_id}: {}", error.detail),
                })?;
            let Some(workout) = envelope.workout else {
                continue;
            };
            let Ok(instant) = workout.start_time.parse::<Timestamp>() else {
                continue;
            };
            let day = zones.started_at(instant).wall_clock().date();
            if within.from.is_some_and(|first| day < first)
                || within.until.is_some_and(|last| day > last)
            {
                continue;
            }

            let record = SourceRecordId::try_from(source_record_id.clone()).map_err(|error| {
                StoreError::Corrupt {
                    detail: error.to_string(),
                }
            })?;

            for entry in &workout.exercises {
                // An unmapped template is not a candidate and is not an error
                // here: the derivation is the thing that fails on a gap in the
                // vocabulary, and this is a question about one exercise.
                //
                // What it reads as is what the derivation reads, so a
                // correction already asserted counts: an entry corrected away
                // from `exercise` is not one, and one corrected to it is.
                let reads = overlay
                    .exercise_for(&record, &entry.exercise_template_id)
                    .or_else(|| lookup(&entry.exercise_template_id).map(|mapped| mapped.exercise));
                if reads != Some(exercise) {
                    continue;
                }
                let Ok(term) = SourceTerm::try_from(entry.exercise_template_id.clone()) else {
                    continue;
                };
                found.push(StandIn {
                    term: CorrectedTerm::new(record.clone(), term),
                    day,
                    title: entry.title.clone(),
                    sets: entry.sets.len(),
                });
            }
        }

        found.sort_by(|left, right| {
            left.day
                .cmp(&right.day)
                .then_with(|| {
                    left.term
                        .record()
                        .as_str()
                        .cmp(right.term.record().as_str())
                })
                .then_with(|| left.term.term().as_str().cmp(right.term.term().as_str()))
        });

        Ok(found)
    }

    /// Every set on `day` recording `recorded`, for a figures correction
    /// (#453), in the order performed.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read or holds a payload that is
    /// not a workout.
    pub async fn recording(
        &self,
        recorded: SetFigures,
        zones: &StatedZones,
        day: Date,
    ) -> Result<Vec<FoundSet>, StoreError> {
        let latest = self.current().await?;

        let mut found = Vec::new();
        for (source_record_id, payload) in latest {
            let envelope =
                WorkoutEnvelope::read(&payload).map_err(|error| StoreError::Corrupt {
                    detail: format!("{source_record_id}: {}", error.detail),
                })?;
            let Some(workout) = envelope.workout else {
                continue;
            };
            let Ok(instant) = workout.start_time.parse::<Timestamp>() else {
                continue;
            };
            let started = zones.started_at(instant).wall_clock();
            if started.date() != day {
                continue;
            }
            let record = SourceRecordId::try_from(source_record_id.clone()).map_err(|error| {
                StoreError::Corrupt {
                    detail: error.to_string(),
                }
            })?;
            for entry in &workout.exercises {
                for set in &entry.sets {
                    if recorded_figures(set) != Some(recorded) {
                        continue;
                    }
                    let Ok(term) =
                        SourceTerm::try_from(set_term(&entry.exercise_template_id, set.index))
                    else {
                        continue;
                    };
                    found.push(FoundSet {
                        set: CorrectedTerm::new(record.clone(), term),
                        movement: Some(entry.title.clone()),
                        began: started.time(),
                        stream: crate::store::HevyWorkoutLandingStore::STREAM,
                    });
                }
            }
        }
        Ok(found)
    }
}
