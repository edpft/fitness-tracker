//! Turning a landed Hevy payload into a gym workout.
//!
//! Deterministic and total: the record's values plus the mapping plus the
//! declared zone resolve the entity with no further input. There is no clock,
//! no request and no overlay in reach, which is what § 9 means and is why this
//! takes `&self` and returns without awaiting anything.
//!
//! **What cannot be expressed is rejected, never coerced.** The grammatical
//! part of a record translates, the ungrammatical part does not, and
//! translation never guesses which repair was meant (§ 37). A refused set does
//! not cost its exercise; a refused exercise does not cost its workout; a
//! refused grouping does not cost its members, which fall back to being
//! ordinary items in the order the record gave them. The one thing that stops
//! the run is a template the mapping does not cover — a defect in our own
//! vocabulary rather than in the data.

use application::{NormalisationError, Translation, ports::WorkoutTranslator};
use domain::{
    gym::{
        Distance, Duration, GymWorkout, Kg, Load, Metres, NonEmpty, OperatorZone,
        PerformedExercise, Refusal, RefusalLocus, RefusalReason, RepCount, Rir, Set, SetKind,
        SignedKg, Superset, TimedDistance, WorkoutItem, WorkoutStart, exercise::Exercise,
        nonempty::AtLeastTwo,
    },
    landing::{EventKind, LandedRecord, LandingRecordId, Provenance, SourceRecordId},
};
use jiff::Timestamp;

use super::{
    mapping::{LoadReading, Mapped, lookup},
    payload::{ExerciseEntry, PerformedSet, WorkoutEnvelope, number},
};

/// The Hevy adapter's translator.
///
/// Stateless. It holds no connection, no cache and no configuration — the zone
/// arrives per call, so the same translator answers for any declared
/// configuration and a test can pin both sides of a switchover without building
/// two of them.
#[derive(Debug, Clone, Copy, Default)]
pub struct HevyWorkoutTranslator;

impl WorkoutTranslator for HevyWorkoutTranslator {
    fn translate(
        &self,
        record: &LandedRecord,
        zone: &OperatorZone,
    ) -> Result<Translation, NormalisationError> {
        let mut scribe = Scribe::new(record);

        // The event kind comes from provenance, which the adapter recorded when
        // the record landed. Reading it again from the body would be a second
        // answer to a question that already has one.
        let Provenance::Event(event) = record.provenance();
        match event.kind() {
            EventKind::Deleted => {
                return Ok(Translation::Retraction {
                    of: record.source_record_id().clone(),
                });
            }
            EventKind::Unrecognised(kind) => {
                return Ok(scribe.only(
                    RefusalLocus::Record,
                    RefusalReason::UnreadablePayload {
                        detail: format!("event kind {kind:?} is not one we translate"),
                    },
                ));
            }
            EventKind::Updated => {}
        }

        let envelope = match WorkoutEnvelope::read(record.payload().as_bytes()) {
            Ok(envelope) => envelope,
            Err(error) => {
                return Ok(scribe.only(
                    RefusalLocus::Record,
                    RefusalReason::UnreadablePayload {
                        detail: error.detail,
                    },
                ));
            }
        };
        let Some(workout) = envelope.workout else {
            return Ok(scribe.only(
                RefusalLocus::Record,
                RefusalReason::UnreadablePayload {
                    detail: "an updated event carrying no workout".to_owned(),
                },
            ));
        };

        let Ok(instant) = workout.start_time.parse::<Timestamp>() else {
            return Ok(scribe.only(
                RefusalLocus::Record,
                RefusalReason::UnreadableValue {
                    field: "start_time",
                    detail: workout.start_time.clone(),
                },
            ));
        };

        let items = Self::items(&workout.exercises, &mut scribe, record)?;
        let Ok(items) = NonEmpty::new(items) else {
            return Ok(scribe.nothing_translatable());
        };

        Ok(Translation::Workout {
            workout: Box::new(GymWorkout::new(
                items,
                WorkoutStart::new(instant, zone.clone()),
                record.provenance().clone(),
                record.source_record_id().clone(),
                record.id(),
            )),
            refusals: scribe.into_refusals(),
        })
    }
}

impl HevyWorkoutTranslator {
    /// The workout's ordered items, with groupings resolved.
    fn items(
        entries: &[ExerciseEntry<'_>],
        scribe: &mut Scribe,
        record: &LandedRecord,
    ) -> Result<Vec<WorkoutItem>, NormalisationError> {
        let grouping = Grouping::read(entries, scribe);

        let mut items = Vec::new();
        let mut pending: Vec<PerformedExercise> = Vec::new();
        let mut pending_group: Option<u32> = None;

        for entry in entries {
            let Some(exercise) = Self::entry(entry, scribe, record)? else {
                // A refused entry ends any run it was part of, because the
                // members either side of it are no longer back to back.
                flush(&mut items, &mut pending, &mut pending_group);
                continue;
            };

            match entry.superset_id.filter(|group| grouping.is_valid(*group)) {
                Some(group) if pending_group == Some(group) => pending.push(exercise),
                Some(group) => {
                    flush(&mut items, &mut pending, &mut pending_group);
                    pending_group = Some(group);
                    pending.push(exercise);
                }
                None => {
                    flush(&mut items, &mut pending, &mut pending_group);
                    items.push(WorkoutItem::Exercise(exercise));
                }
            }
        }
        flush(&mut items, &mut pending, &mut pending_group);

        Ok(items)
    }

    /// One exercise entry with its sets, or nothing if none of them survived.
    fn entry(
        entry: &ExerciseEntry<'_>,
        scribe: &mut Scribe,
        record: &LandedRecord,
    ) -> Result<Option<PerformedExercise>, NormalisationError> {
        let Some(mapped) = lookup(&entry.exercise_template_id) else {
            // The one failure that is ours rather than the data's. Naming the
            // identifier is the whole point: it is a gap in the vocabulary to
            // go and fill, so nothing translates around it.
            return Err(NormalisationError::UnmappedExercise {
                template_id: entry.exercise_template_id.clone(),
                source_record_id: record.source_record_id().as_str().to_owned(),
            });
        };

        if entry.sets.is_empty() {
            scribe.note_for(
                RefusalLocus::Entry { entry: entry.index },
                Some(mapped.exercise),
                RefusalReason::NoSetsInEntry,
            );
            return Ok(None);
        }

        Ok(Self::sets(entry, mapped, scribe))
    }

    /// The entry's sets, in the measure its exercise is counted in.
    ///
    /// Four arms rather than one, because a `Set<RepCount>` and a
    /// `Set<Duration>` are different types — which is the partition doing its
    /// work, and is what makes a set that disagrees with its exercise
    /// impossible to build rather than something to check for.
    fn sets(
        entry: &ExerciseEntry<'_>,
        mapped: Mapped,
        scribe: &mut Scribe,
    ) -> Option<PerformedExercise> {
        match mapped.exercise {
            Exercise::Reps(exercise) => {
                let sets = Self::collect(entry, mapped, scribe, reps_of);
                NonEmpty::new(sets)
                    .ok()
                    .map(|sets| PerformedExercise::ForReps { exercise, sets })
            }
            Exercise::Duration(exercise) => {
                let sets = Self::collect(entry, mapped, scribe, duration_of);
                NonEmpty::new(sets)
                    .ok()
                    .map(|sets| PerformedExercise::ForDuration { exercise, sets })
            }
            Exercise::Distance(exercise) => {
                let sets = Self::collect(entry, mapped, scribe, distance_of);
                NonEmpty::new(sets)
                    .ok()
                    .map(|sets| PerformedExercise::ForDistance { exercise, sets })
            }
            Exercise::TimedDistance(exercise) => {
                let sets = Self::collect(entry, mapped, scribe, timed_distance_of);
                NonEmpty::new(sets)
                    .ok()
                    .map(|sets| PerformedExercise::ForTimedDistance { exercise, sets })
            }
        }
    }

    /// Every set of an entry that translates, with the rest noted.
    ///
    /// Generic over how the measure is read, so the load, the intensity and the
    /// kind — which are the same questions whatever a set is counted in — are
    /// asked once here rather than four times above.
    fn collect<M>(
        entry: &ExerciseEntry<'_>,
        mapped: Mapped,
        scribe: &mut Scribe,
        measure_of: fn(&PerformedSet<'_>) -> Result<M, RefusalReason>,
    ) -> Vec<Set<M>> {
        let mut translated = Vec::with_capacity(entry.sets.len());
        for set in &entry.sets {
            let locus = RefusalLocus::Set {
                entry: entry.index,
                set: set.index,
            };
            match Self::one_set(set, mapped, measure_of) {
                Ok(set) => translated.push(set),
                Err(reason) => scribe.note_for(locus, Some(mapped.exercise), reason),
            }
        }
        translated
    }

    fn one_set<M>(
        set: &PerformedSet<'_>,
        mapped: Mapped,
        measure_of: fn(&PerformedSet<'_>) -> Result<M, RefusalReason>,
    ) -> Result<Set<M>, RefusalReason> {
        Ok(Set {
            load: load_of(set, mapped.load)?,
            measure: measure_of(set)?,
            intensity: intensity_of(set)?,
            kind: kind_of(&set.kind)?,
            // Hevy's logged set carries no rest field and no per-set
            // timestamps, and reconstructing it from a linked routine would be
            // prescription masquerading as observation (§ 11). Permanently
            // absent from this adapter, which is § 37 working rather than a gap.
            rest_after: None,
        })
    }
}

/// What a source called a set, in our two kinds.
///
/// `failure` and `dropset` are both working sets to the only question asked of
/// the field. A set taken to failure is zero reps in reserve, which is the
/// reliable signal anyway — the flag was used inconsistently and abandoned. An
/// unrecognised kind refuses rather than defaulting, because defaulting would
/// silently file a set the source meant something else by.
fn kind_of(kind: &str) -> Result<SetKind, RefusalReason> {
    match kind {
        "normal" | "failure" | "dropset" => Ok(SetKind::Working),
        "warmup" => Ok(SetKind::Warmup),
        other => Err(RefusalReason::UnknownSetKind {
            kind: other.to_owned(),
        }),
    }
}

/// Hevy's RPE onto our reps in reserve.
///
/// Hevy glosses RPE as reps in reserve in its own interface and it is used that
/// way, which makes reps in reserve the recorded fact. The mapping is total
/// across the eight positions the source offers; anything else refuses.
///
/// Matched on the number's own characters, so `9.5` never becomes a float on
/// its way to a comparison.
fn intensity_of(set: &PerformedSet<'_>) -> Result<Option<Rir>, RefusalReason> {
    let Some(token) = number(set.rpe) else {
        // Absent is absent: not zero, and not carried forward from a neighbour.
        return Ok(None);
    };
    match token {
        "10" | "10.0" => Ok(Some(Rir::Zero)),
        "9.5" => Ok(Some(Rir::ZeroOrOne)),
        "9" | "9.0" => Ok(Some(Rir::One)),
        "8.5" => Ok(Some(Rir::OneOrTwo)),
        "8" | "8.0" => Ok(Some(Rir::Two)),
        "7.5" => Ok(Some(Rir::TwoOrThree)),
        "7" | "7.0" => Ok(Some(Rir::Three)),
        "6" | "6.0" => Ok(Some(Rir::FourOrMore)),
        other => Err(RefusalReason::UnrecognisedIntensity {
            value: other.to_owned(),
        }),
    }
}

/// The weight column, read the way this exercise's mapping says to.
fn load_of(set: &PerformedSet<'_>, reading: LoadReading) -> Result<Load, RefusalReason> {
    let token = number(set.weight_kg);
    match reading {
        LoadReading::BandResistance => Err(RefusalReason::BandResistance),
        LoadReading::Absolute => {
            let Some(token) = token else {
                return Err(RefusalReason::UnreadableValue {
                    field: "weight_kg",
                    detail: "absent on an exercise whose implement has mass".to_owned(),
                });
            };
            let mass = Kg::try_from(token).map_err(|error| RefusalReason::UnreadableValue {
                field: "weight_kg",
                detail: error.to_string(),
            })?;
            // No bar mass is assumed and no default applied: 10, 15 and 20 kg
            // bars are all in use, so every repair is a guess.
            Load::absolute(mass).map_err(|_| RefusalReason::ZeroOnAbsoluteLoad)
        }
        LoadReading::Relative | LoadReading::RelativeNegated => {
            // An absent weight on a relative exercise is plain bodyweight,
            // which is a real observation rather than a missing one.
            let Some(token) = token else {
                return Ok(Load::BODYWEIGHT);
            };
            let delta =
                SignedKg::try_from(token).map_err(|error| RefusalReason::UnreadableValue {
                    field: "weight_kg",
                    detail: error.to_string(),
                })?;
            Ok(Load::relative(if reading == LoadReading::RelativeNegated {
                delta.negated()
            } else {
                delta
            }))
        }
    }
}

fn reps_of(set: &PerformedSet<'_>) -> Result<RepCount, RefusalReason> {
    let Some(reps) = set.reps else {
        return Err(RefusalReason::UnreadableValue {
            field: "reps",
            detail: "absent on an exercise counted in repetitions".to_owned(),
        });
    };
    // Zero is the one genuine gap: a rep attempted and missed is a real event
    // and is not a set, and it needs an attempt rather than a weaker RepCount.
    RepCount::new(reps).map_err(|_| RefusalReason::ZeroReps)
}

fn duration_of(set: &PerformedSet<'_>) -> Result<Duration, RefusalReason> {
    set.duration_seconds
        .map(Duration::from_seconds)
        .ok_or_else(|| RefusalReason::UnreadableValue {
            field: "duration_seconds",
            detail: "absent on an exercise counted in elapsed time".to_owned(),
        })
}

fn metres_of(set: &PerformedSet<'_>) -> Result<Metres, RefusalReason> {
    let Some(token) = number(set.distance_meters) else {
        return Err(RefusalReason::UnreadableValue {
            field: "distance_meters",
            detail: "absent on an exercise counted in ground covered".to_owned(),
        });
    };
    Metres::try_from(token).map_err(|error| RefusalReason::UnreadableValue {
        field: "distance_meters",
        detail: error.to_string(),
    })
}

fn distance_of(set: &PerformedSet<'_>) -> Result<Distance, RefusalReason> {
    metres_of(set).map(|metres| Distance { metres })
}

fn timed_distance_of(set: &PerformedSet<'_>) -> Result<TimedDistance, RefusalReason> {
    Ok(TimedDistance {
        metres: metres_of(set)?,
        duration: duration_of(set)?,
    })
}

/// Which of a workout's groupings are supersets, and which are malformed.
///
/// A superset is exercises performed back to back, so its members are
/// contiguous and there are at least two of them. Translation cannot repair a
/// record that breaks this, because every repair is a guess — so the grouping
/// does not translate, the omission is recorded, and its members become
/// ordinary items in their recorded order. The workout is not lost to a bad
/// grouping.
struct Grouping {
    valid: Vec<u32>,
}

impl Grouping {
    fn read(entries: &[ExerciseEntry<'_>], scribe: &mut Scribe) -> Self {
        let mut valid = Vec::new();
        let mut seen: Vec<u32> = Vec::new();

        for entry in entries {
            let Some(group) = entry.superset_id else {
                continue;
            };
            if seen.contains(&group) {
                continue;
            }
            seen.push(group);

            let positions: Vec<usize> = entries
                .iter()
                .enumerate()
                .filter(|(_, other)| other.superset_id == Some(group))
                .map(|(position, _)| position)
                .collect();

            let locus = RefusalLocus::Grouping { group };
            match positions.as_slice() {
                [] | [_] => scribe.note(locus, RefusalReason::SingleMemberGrouping),
                [first, .., last] if last.saturating_sub(*first) + 1 == positions.len() => {
                    valid.push(group);
                }
                _ => scribe.note(locus, RefusalReason::NonContiguousGrouping),
            }
        }

        Self { valid }
    }

    fn is_valid(&self, group: u32) -> bool {
        self.valid.contains(&group)
    }
}

/// Close off a run of grouped exercises.
///
/// Two or more make a superset. One means the grouping was valid when it was
/// read but has lost a member to a refusal since — so it degrades to an
/// ordinary item and the exercise that did translate is kept. Dropping it would
/// cost a good observation to a neighbour's bad one, which is the opposite of
/// what refusal is for. No refusal is noted: the member that went is already
/// recorded, and the grouping is not itself malformed.
fn flush(
    items: &mut Vec<WorkoutItem>,
    pending: &mut Vec<PerformedExercise>,
    group: &mut Option<u32>,
) {
    *group = None;
    let mut members = std::mem::take(pending).into_iter();
    match (members.next(), members.next()) {
        (Some(first), Some(second)) => items.push(WorkoutItem::Superset(Superset {
            members: AtLeastTwo::of(first, second, members.collect()),
        })),
        (Some(only), None) => items.push(WorkoutItem::Exercise(only)),
        _ => {}
    }
}

/// Collects refusals as translation walks a record.
///
/// A small mutable thing rather than a returned list at every level: a refusal
/// can be raised four layers down, and threading `Vec<Refusal>` through each of
/// them would put the plumbing in front of the reading.
struct Scribe {
    landed_as: LandingRecordId,
    source_record_id: SourceRecordId,
    refusals: Vec<Refusal>,
}

impl Scribe {
    fn new(record: &LandedRecord) -> Self {
        Self {
            landed_as: record.id(),
            source_record_id: record.source_record_id().clone(),
            refusals: Vec::new(),
        }
    }

    fn note(&mut self, locus: RefusalLocus, reason: RefusalReason) {
        self.note_for(locus, None, reason);
    }

    /// A refusal that knows which exercise it belonged to.
    fn note_for(&mut self, locus: RefusalLocus, exercise: Option<Exercise>, reason: RefusalReason) {
        self.refusals.push(Refusal {
            landed_as: self.landed_as,
            source_record_id: self.source_record_id.clone(),
            locus,
            exercise,
            reason,
        });
    }

    /// A record that produced nothing but this one reason.
    fn only(&mut self, locus: RefusalLocus, reason: RefusalReason) -> Translation {
        self.note(locus, reason);
        self.nothing_translatable()
    }

    /// Every item refused, so the record yields no workout — a workout holds a
    /// non-empty sequence of items by construction. Not a run failure.
    fn nothing_translatable(&mut self) -> Translation {
        if self.refusals.is_empty() {
            self.note(RefusalLocus::Record, RefusalReason::NothingTranslatable);
        }
        let refusals = std::mem::take(&mut self.refusals);
        NonEmpty::new(refusals).map_or_else(
            |_| {
                Translation::Refused(NonEmpty::of(
                    Refusal {
                        landed_as: self.landed_as,
                        source_record_id: self.source_record_id.clone(),
                        locus: RefusalLocus::Record,
                        exercise: None,
                        reason: RefusalReason::NothingTranslatable,
                    },
                    Vec::new(),
                ))
            },
            Translation::Refused,
        )
    }

    fn into_refusals(self) -> Vec<Refusal> {
        self.refusals
    }
}
