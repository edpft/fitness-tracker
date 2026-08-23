//! What was issued (§ 12).
//!
//! Written once and never rewritten. The anchor and the parameters are recorded
//! by value, which is what makes § 14's "only the current value is required"
//! true: a superseded percentage answers no question because what it produced is
//! here.
//!
//! `(issued_for, issued_at)` is the unique key, so a date may be prescribed
//! more than once and the greatest `issued_at` is the one in force — the same
//! rule `generation_parameters` uses, and for the same reason: an issued
//! prescription is authored data (§ 12) and keeps its history. FR-010's
//! idempotence is therefore the schema's rather than a caller's to remember.

use application::{PrescribedWorkoutId, PrescribedWorkoutStore, StoreError};
use domain::{
    gym::{
        Distance, Duration, Kg, Load, Metres, NonEmpty, RepCount, Rir, SignedKg, Spans,
        exercise::{DistanceExercise, DurationExercise, RepsExercise},
        sequence::AtLeastTwo,
    },
    prescription::{
        Anchor, AnchorProvenance, DerivedFrom, GenerationParameters, Prescribed,
        PrescribedExercise, PrescribedItem, PrescribedSet, PrescribedSuperset, PrescribedWorkout,
        ProgrammeId, SessionRole, SlotId, SupersetMember, Target, WeekIndex, WeekKind,
        WorkoutShape,
    },
};
use jiff::civil::Date;
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{corrupt, store_error};

#[derive(Debug, Clone)]
pub struct SqlitePrescribedWorkoutStore {
    pool: SqlitePool,
    /// The zone every issued date is recorded against. Configuration rather than
    /// programme data (§ II.3), so it arrives with the adapter.
    zone: String,
}

impl SqlitePrescribedWorkoutStore {
    pub const fn new(pool: SqlitePool, zone: String) -> Self {
        Self { pool, zone }
    }
}

/// A load on its way into the store.
fn load_for_storage(load: Load) -> Result<(&'static str, i64), StoreError> {
    Ok(match load {
        Load::Absolute(mass) => (
            "absolute",
            i64::try_from(mass.as_grams())
                .map_err(|_| corrupt(&"a load larger than the store can hold"))?,
        ),
        Load::Relative(delta) => ("relative", delta.as_grams()),
    })
}

fn load_from_storage(kind: &str, grams: i64) -> Result<Load, StoreError> {
    match kind {
        "absolute" => {
            let unsigned = u64::try_from(grams)
                .map_err(|_| corrupt(&"an absolute load stored as a negative mass"))?;
            Ok(Load::Absolute(Kg::from_grams(unsigned)))
        }
        "relative" => Ok(Load::Relative(SignedKg::from_grams(grams))),
        other => Err(corrupt(&format!("{other:?} is not a load kind"))),
    }
}

/// The measure columns of a `Target<M>`, flattened.
struct TargetColumns {
    kind: &'static str,
    low: i64,
    high: Option<i64>,
}

fn reps_target(target: Target<RepCount>) -> TargetColumns {
    match target {
        Target::Exactly(reps) => TargetColumns {
            kind: "reps",
            low: i64::from(reps.as_u32()),
            high: None,
        },
        range @ Target::Range { .. } => TargetColumns {
            kind: "reps",
            low: i64::from(range.minimum().as_u32()),
            high: Some(i64::from(range.maximum().as_u32())),
        },
    }
}

fn duration_target(target: Target<Duration>) -> Result<TargetColumns, StoreError> {
    let seconds = |value: Duration| {
        i64::try_from(value.as_seconds())
            .map_err(|_| corrupt(&"a duration longer than the store can hold"))
    };
    Ok(match target {
        Target::Exactly(value) => TargetColumns {
            kind: "duration",
            low: seconds(value)?,
            high: None,
        },
        range @ Target::Range { .. } => TargetColumns {
            kind: "duration",
            low: seconds(range.minimum())?,
            high: Some(seconds(range.maximum())?),
        },
    })
}

fn distance_target(target: Target<Distance>) -> Result<TargetColumns, StoreError> {
    let mm = |value: Distance| {
        i64::try_from(value.metres.as_millimetres())
            .map_err(|_| corrupt(&"a distance larger than the store can hold"))
    };
    Ok(match target {
        Target::Exactly(value) => TargetColumns {
            kind: "distance",
            low: mm(value)?,
            high: None,
        },
        range @ Target::Range { .. } => TargetColumns {
            kind: "distance",
            low: mm(range.minimum())?,
            high: Some(mm(range.maximum())?),
        },
    })
}

/// The columns recording what a session's primary loads came from.
///
/// **One of the two, never both.** A session issued from a programme that climbs
/// derives its loads from an anchor; one issued from a standalone test derives
/// them from the target the record put it at, and that programme has no anchor.
/// Migration 0016's `CHECK` says the same thing from the other side.
struct Origin {
    anchor_grams: Option<i64>,
    provenance: Option<&'static str>,
    anchor_from: Option<String>,
    anchor_failed: Option<i64>,
    target_grams: Option<i64>,
}

/// What a stored session derived from, from the columns that recorded it.
///
/// **Neither or both is corrupt, not a default.** The schema refuses either, so
/// a row reaching here with both set got past the database and saying so is more
/// use than picking one.
///
/// The provenance travels separately because it is a stored string on the way in
/// and a `&'static str` on the way out.
fn read_origin(origin: Origin, provenance: Option<String>) -> Result<DerivedFrom, StoreError> {
    match (origin.anchor_grams, origin.target_grams) {
        (Some(grams), None) => {
            let failed = origin
                .anchor_failed
                .map(|grams| {
                    u64::try_from(grams)
                        .map(Kg::from_grams)
                        .map_err(|_| corrupt(&"a failed load stored as a negative mass"))
                })
                .transpose()?;
            let provenance = provenance.ok_or_else(|| corrupt(&"an anchor from nowhere"))?;
            let from = origin
                .anchor_from
                .ok_or_else(|| corrupt(&"an anchor with no date"))?
                .parse::<Date>()
                .map_err(|_| corrupt(&"an anchor date that is not a date"))?;
            Ok(DerivedFrom::Anchor(
                Anchor::new(
                    u64::try_from(grams)
                        .map(Kg::from_grams)
                        .map_err(|_| corrupt(&"an anchor stored as a negative mass"))?,
                    failed,
                    AnchorProvenance::try_from(provenance).map_err(|error| corrupt(&error))?,
                    from,
                )
                .map_err(|error| corrupt(&error))?,
            ))
        }
        (None, Some(grams)) => Ok(DerivedFrom::Target(
            u64::try_from(grams)
                .map(Kg::from_grams)
                .map_err(|_| corrupt(&"a target stored as a negative mass"))?,
        )),
        (Some(_), Some(_)) => Err(corrupt(
            &"a prescription derived from both an anchor and a target",
        )),
        (None, None) => Err(corrupt(&"a prescription derived from nothing")),
    }
}

fn origin_of(derived_from: DerivedFrom) -> Result<Origin, StoreError> {
    let anchor = derived_from.anchor();
    Ok(Origin {
        anchor_grams: anchor
            .map(|anchor| {
                i64::try_from(anchor.load().as_grams())
                    .map_err(|_| corrupt(&"an anchor larger than the store can hold"))
            })
            .transpose()?,
        provenance: anchor.map(|anchor| anchor.provenance().as_str()),
        anchor_from: anchor.map(|anchor| anchor.from().to_string()),
        anchor_failed: anchor
            .and_then(Anchor::failed)
            .map(|failed| {
                i64::try_from(failed.as_grams())
                    .map_err(|_| corrupt(&"a failed load larger than the store can hold"))
            })
            .transpose()?,
        target_grams: derived_from
            .target()
            .map(|target| {
                i64::try_from(target.as_grams())
                    .map_err(|_| corrupt(&"a target larger than the store can hold"))
            })
            .transpose()?,
    })
}

/// One prescribed set, flattened.
struct SetColumns {
    variant: &'static str,
    load_kind: Option<&'static str>,
    load_grams: Option<i64>,
    target: Option<TargetColumns>,
    effort: Option<String>,
    warmup: i64,
}

fn set_columns<M>(
    set: &PrescribedSet<M>,
    target_of: impl Fn(Target<M>) -> Result<TargetColumns, StoreError>,
) -> Result<SetColumns, StoreError>
where
    M: Copy + Spans,
{
    let (load_kind, load_grams) = match set.prescription.load() {
        Some(load) => {
            let (kind, grams) = load_for_storage(load)?;
            (Some(kind), Some(grams))
        }
        None => (None, None),
    };
    let target = match set.prescription.measure() {
        Some(measure) => Some(target_of(*measure)?),
        None => None,
    };
    Ok(SetColumns {
        variant: set.prescription.as_str(),
        load_kind,
        load_grams,
        target,
        effort: set.prescription.effort().map(|rir| rir.as_str().to_owned()),
        warmup: i64::from(set.warmup),
    })
}

async fn write_set(
    tx: &mut Transaction<'_, Sqlite>,
    workout: i64,
    item: i64,
    exercise: i64,
    position: i64,
    columns: &SetColumns,
) -> Result<(), StoreError> {
    let (target_kind, target_low, target_high) = columns
        .target
        .as_ref()
        .map_or((None, None, None), |target| {
            (Some(target.kind), Some(target.low), target.high)
        });
    sqlx::query!(
        r"
        INSERT INTO prescribed_set (
            workout, item_position, exercise_position, position,
            variant, load_kind, load_grams,
            target_kind, target_low, target_high,
            effort, rest_low_seconds, rest_high_seconds, warmup
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, ?)
        ",
        workout,
        item,
        exercise,
        position,
        columns.variant,
        columns.load_kind,
        columns.load_grams,
        target_kind,
        target_low,
        target_high,
        columns.effort,
        columns.warmup
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(())
}

/// One exercise and its sets.
async fn write_exercise(
    tx: &mut Transaction<'_, Sqlite>,
    workout: i64,
    item: i64,
    position: i64,
    exercise: &PrescribedExercise,
) -> Result<(), StoreError> {
    let key = exercise.exercise_key();
    let measure = exercise.measure();
    sqlx::query!(
        r"
        INSERT INTO prescribed_exercise (workout, item_position, position, exercise, measure)
        VALUES (?, ?, ?, ?, ?)
        ",
        workout,
        item,
        position,
        key,
        measure
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    match exercise {
        PrescribedExercise::ForReps { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let columns = set_columns(set, |target| Ok(reps_target(target)))?;
                write_set(tx, workout, item, position, ordinal_of(ordinal)?, &columns).await?;
            }
        }
        PrescribedExercise::ForDuration { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let columns = set_columns(set, duration_target)?;
                write_set(tx, workout, item, position, ordinal_of(ordinal)?, &columns).await?;
            }
        }
        PrescribedExercise::ForDistance { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let columns = set_columns(set, distance_target)?;
                write_set(tx, workout, item, position, ordinal_of(ordinal)?, &columns).await?;
            }
        }
    }
    Ok(())
}

fn ordinal_of(position: usize) -> Result<i64, StoreError> {
    i64::try_from(position).map_err(|_| corrupt(&"more sets than the store can hold"))
}

impl PrescribedWorkoutStore for SqlitePrescribedWorkoutStore {
    async fn issue(&self, workout: &PrescribedWorkout) -> Result<PrescribedWorkoutId, StoreError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        let programme = workout.programme().as_i64();
        let issued_for = workout.issued_for().to_string();
        let role = workout.session_role().as_str();
        let week_kind = workout.week().as_str();
        let week_index = workout
            .week()
            .index()
            .map(|index| i64::from(index.as_u32()));
        let Origin {
            anchor_grams,
            provenance,
            anchor_from,
            anchor_failed,
            target_grams,
        } = origin_of(workout.derived_from())?;
        // The version the parameters came from — the join back to the authored
        // set. The values themselves are on the row too, which is what § 14
        // rests on; this is what tells two versions apart.
        let parameters_at = workout.parameters_authored_at().to_string();
        let issued_at = workout.issued_at().to_string();

        let id = sqlx::query!(
            r#"
            INSERT INTO prescribed_workout (
                programme, issued_for, zone, session_role,
                week_kind, week_index,
                anchor_grams, anchor_provenance, anchor_from, anchor_failed_grams,
                target_grams, parameters_authored_at, issued_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id AS "id!: i64"
            "#,
            programme,
            issued_for,
            self.zone,
            role,
            week_kind,
            week_index,
            anchor_grams,
            provenance,
            anchor_from,
            anchor_failed,
            target_grams,
            parameters_at,
            issued_at
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|error| store_error(&error))?
        .id;

        for (position, item) in workout.shape().items().iter().enumerate() {
            let position = ordinal_of(position)?;
            let is_superset = i64::from(matches!(item, PrescribedItem::Superset(_)));
            sqlx::query!(
                r"
                INSERT INTO prescribed_item (workout, position, is_superset)
                VALUES (?, ?, ?)
                ",
                id,
                position,
                is_superset
            )
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

            for (member, slot) in item.slots().enumerate() {
                let member = ordinal_of(member)?;
                let slot = slot.as_str();
                sqlx::query!(
                    r"
                    INSERT INTO prescribed_slot (workout, item_position, member_position, slot)
                    VALUES (?, ?, ?, ?)
                    ",
                    id,
                    position,
                    member,
                    slot
                )
                .execute(&mut *tx)
                .await
                .map_err(|error| store_error(&error))?;
            }

            for (ordinal, exercise) in item.exercises().enumerate() {
                write_exercise(&mut tx, id, position, ordinal_of(ordinal)?, exercise).await?;
            }
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(PrescribedWorkoutId::new(id))
    }

    async fn issued_for(
        &self,
        date: Date,
    ) -> Result<Option<(PrescribedWorkoutId, PrescribedWorkout)>, StoreError> {
        let key = date.to_string();
        let Some(row) = sqlx::query!(
            r#"
            SELECT id AS "id!: i64", programme AS "programme!: i64",
                   session_role AS "session_role!: String",
                   week_kind AS "week_kind!: String", week_index AS "week_index: i64",
                   anchor_grams AS "anchor_grams: i64",
                   anchor_provenance AS "anchor_provenance: String",
                   anchor_from AS "anchor_from: String",
                   anchor_failed_grams AS "anchor_failed_grams: i64",
                   target_grams AS "target_grams: i64",
                   parameters_authored_at AS "parameters_authored_at!: String",
                   issued_at AS "issued_at!: String"
            FROM prescribed_workout
            WHERE issued_for = ?
            -- The latest issue is the one in force. A date may be prescribed
            -- more than once and a correction supersedes rather than
            -- overwrites, so the superseded rows are still here and still
            -- exactly as they were issued.
            ORDER BY issued_at DESC
            LIMIT 1
            "#,
            key
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?
        else {
            return Ok(None);
        };

        let shape = read_shape(&self.pool, row.id).await?;

        let derived_from = read_origin(
            Origin {
                anchor_grams: row.anchor_grams,
                provenance: None,
                anchor_from: row.anchor_from,
                anchor_failed: row.anchor_failed_grams,
                target_grams: row.target_grams,
            },
            row.anchor_provenance,
        )?;

        let week = match row.week_kind.as_str() {
            "test" => WeekKind::Test,
            "climbing" => {
                let index = row
                    .week_index
                    .ok_or_else(|| corrupt(&"a climbing week with no index"))?;
                let index = u32::try_from(index)
                    .map_err(|_| corrupt(&"a week index the domain cannot hold"))?;
                WeekKind::Climbing(WeekIndex::new(index).map_err(|error| corrupt(&error))?)
            }
            other => return Err(corrupt(&format!("{other:?} is not a week kind"))),
        };

        // The parameters that produced this prescription are read from the
        // version it names. They are what § 14 depends on being recoverable.
        let parameters_at: jiff::Timestamp = row
            .parameters_authored_at
            .parse()
            .map_err(|_| corrupt(&"a parameter version that is not an instant"))?;
        let parameters = read_parameters(&self.pool, &row.parameters_authored_at).await?;

        let workout = PrescribedWorkout::new(
            shape,
            date,
            SessionRole::try_from(row.session_role).map_err(|error| corrupt(&error))?,
            week,
            derived_from,
            parameters,
            parameters_at,
            ProgrammeId::new(row.programme),
            row.issued_at
                .parse()
                .map_err(|_| corrupt(&"an issue time that is not an instant"))?,
        );
        Ok(Some((PrescribedWorkoutId::new(row.id), workout)))
    }
}

/// The parameters a prescription was generated against.
async fn read_parameters(
    pool: &SqlitePool,
    authored_at: &str,
) -> Result<GenerationParameters, StoreError> {
    // Deferring to the parameter store rather than re-reading the tables keeps
    // one assembly of a `GenerationParameters` in the codebase. It reads the
    // current version, which is the one an issued prescription names as long as
    // nothing has been authored since — and where something has, the version on
    // the row is what distinguishes them.
    let store = super::parameters::SqliteGenerationParameterStore::new(pool.clone());
    let (_, parameters) = application::GenerationParameterStore::current(&store)
        .await?
        .ok_or_else(|| corrupt(&format!("no parameters authored at {authored_at}")))?;
    Ok(parameters)
}

/// The issued shape, rebuilt from its four tables.
async fn read_shape(pool: &SqlitePool, workout: i64) -> Result<WorkoutShape, StoreError> {
    let items = sqlx::query!(
        r#"
        SELECT position AS "position!: i64", is_superset AS "is_superset!: i64"
        FROM prescribed_item WHERE workout = ? ORDER BY position ASC
        "#,
        workout
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut built = Vec::new();
    for item in items {
        let slots = sqlx::query!(
            r#"
            SELECT slot AS "slot!: String"
            FROM prescribed_slot
            WHERE workout = ? AND item_position = ?
            ORDER BY member_position ASC
            "#,
            workout,
            item.position
        )
        .fetch_all(pool)
        .await
        .map_err(|error| store_error(&error))?;

        let exercises = read_exercises(pool, workout, item.position).await?;

        let mut slot_ids = Vec::with_capacity(slots.len());
        for slot in slots {
            slot_ids.push(SlotId::try_from(slot.slot).map_err(|error| corrupt(&error))?);
        }

        if item.is_superset == 0 {
            let (Some(slot), Some(exercise)) =
                (slot_ids.into_iter().next(), exercises.into_iter().next())
            else {
                return Err(corrupt(&"a single item with no exercise"));
            };
            built.push(PrescribedItem::Exercise { slot, exercise });
        } else {
            let mut members = slot_ids
                .into_iter()
                .zip(exercises)
                .map(|(slot, exercise)| SupersetMember { slot, exercise });
            let (Some(first), Some(second)) = (members.next(), members.next()) else {
                return Err(corrupt(&"a superset with fewer than two members"));
            };
            built.push(PrescribedItem::Superset(PrescribedSuperset {
                members: AtLeastTwo::of(first, second, members.collect()),
            }));
        }
    }

    let items = NonEmpty::new(built).map_err(|_| corrupt(&"a prescription with no items"))?;
    Ok(WorkoutShape::new(items))
}

async fn read_exercises(
    pool: &SqlitePool,
    workout: i64,
    item: i64,
) -> Result<Vec<PrescribedExercise>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT position AS "position!: i64", exercise AS "exercise!: String",
               measure AS "measure!: String"
        FROM prescribed_exercise
        WHERE workout = ? AND item_position = ?
        ORDER BY position ASC
        "#,
        workout,
        item
    )
    .fetch_all(pool)
    .await
    .map_err(|error| store_error(&error))?;

    let mut built = Vec::with_capacity(rows.len());
    for row in rows {
        let sets = sqlx::query!(
            r#"
            SELECT variant AS "variant!: String", load_kind AS "load_kind: String",
                   load_grams AS "load_grams: i64", target_low AS "target_low: i64",
                   target_high AS "target_high: i64", effort AS "effort: String",
                   warmup AS "warmup!: i64"
            FROM prescribed_set
            WHERE workout = ? AND item_position = ? AND exercise_position = ?
            ORDER BY position ASC
            "#,
            workout,
            item,
            row.position
        )
        .fetch_all(pool)
        .await
        .map_err(|error| store_error(&error))?;

        let sets: Vec<SetRow> = sets
            .into_iter()
            .map(|set| SetRow {
                variant: set.variant,
                load_kind: set.load_kind,
                load_grams: set.load_grams,
                target_low: set.target_low,
                target_high: set.target_high,
                effort: set.effort,
                warmup: set.warmup,
            })
            .collect();

        built.push(rebuild_exercise(&row.exercise, &row.measure, &sets)?);
    }
    Ok(built)
}

/// One exercise, rebuilt in whichever measure its vocabulary fixes.
///
/// Three arms rather than one, because a `PrescribedSet<RepCount>` and a
/// `PrescribedSet<Duration>` are different types — the same partition the
/// performed side has, doing the same work.
fn rebuild_exercise(
    key: &str,
    measure: &str,
    sets: &[SetRow],
) -> Result<PrescribedExercise, StoreError> {
    let empty = || corrupt(&"an exercise with no sets");
    match measure {
        "reps" => {
            let mut built = Vec::with_capacity(sets.len());
            for set in sets {
                built.push(rebuild(set, |low, high| {
                    span(low, high, |value| {
                        u32::try_from(value)
                            .ok()
                            .and_then(|count| RepCount::new(count).ok())
                            .ok_or_else(|| corrupt(&"a repetition count the domain cannot hold"))
                    })
                })?);
            }
            Ok(PrescribedExercise::ForReps {
                exercise: RepsExercise::try_from(key.to_owned())
                    .map_err(|error| corrupt(&error))?,
                sets: NonEmpty::new(built).map_err(|_| empty())?,
            })
        }
        "duration" => {
            let mut built = Vec::with_capacity(sets.len());
            for set in sets {
                built.push(rebuild(set, |low, high| {
                    span(low, high, |value| {
                        u64::try_from(value)
                            .map(Duration::from_seconds)
                            .map_err(|_| corrupt(&"a negative duration"))
                    })
                })?);
            }
            Ok(PrescribedExercise::ForDuration {
                exercise: DurationExercise::try_from(key.to_owned())
                    .map_err(|error| corrupt(&error))?,
                sets: NonEmpty::new(built).map_err(|_| empty())?,
            })
        }
        "distance" => {
            let mut built = Vec::with_capacity(sets.len());
            for set in sets {
                built.push(rebuild(set, |low, high| {
                    span(low, high, |value| {
                        u64::try_from(value)
                            .map(|mm| Distance {
                                metres: Metres::from_millimetres(mm),
                            })
                            .map_err(|_| corrupt(&"a negative distance"))
                    })
                })?);
            }
            Ok(PrescribedExercise::ForDistance {
                exercise: DistanceExercise::try_from(key.to_owned())
                    .map_err(|error| corrupt(&error))?,
                sets: NonEmpty::new(built).map_err(|_| empty())?,
            })
        }
        other => Err(corrupt(&format!("{other:?} is not a measure"))),
    }
}

/// A `Target<M>` from its two columns. `high` absent means `Exactly`.
///
/// **The schema stores bounds and the domain holds a span**, so this is a
/// boundary rather than a translation: `Target::between` is the only fallible
/// way to build a range, and it exists for exactly this — reading back a pair
/// that something outside the domain wrote down. The column check should have
/// refused a non-spanning pair already, so the error arm means a corrupt row.
fn span<M: Spans>(
    low: i64,
    high: Option<i64>,
    of: impl Fn(i64) -> Result<M, StoreError>,
) -> Result<Target<M>, StoreError> {
    match high {
        Some(high) => Target::between(of(low)?, of(high)?)
            .ok_or_else(|| corrupt(&"a stored range that does not span")),
        None => Ok(Target::Exactly(of(low)?)),
    }
}

/// One set row back into a `PrescribedSet<M>`.
///
/// The variant decides which columns mean anything, which is the same rule the
/// schema's `CHECK` constraints hold. Reading it back through the variant rather
/// than "whichever column is filled" is what keeps the two in agreement.
fn rebuild<M: Spans>(
    row: &SetRow,
    target_of: impl Fn(i64, Option<i64>) -> Result<Target<M>, StoreError>,
) -> Result<PrescribedSet<M>, StoreError> {
    let load = match (row.load_kind.as_deref(), row.load_grams) {
        (Some(kind), Some(grams)) => Some(load_from_storage(kind, grams)?),
        _ => None,
    };
    let effort = match row.effort.as_deref() {
        Some(key) => Some(Rir::try_from(key.to_owned()).map_err(|error| corrupt(&error))?),
        None => None,
    };
    let target = match row.target_low {
        Some(low) => Some(target_of(low, row.target_high)?),
        None => None,
    };

    let prescription = match row.variant.as_str() {
        "fixed" => {
            let (Some(load), Some(measure)) = (load, target) else {
                return Err(corrupt(&"a fixed set with no load or no measure"));
            };
            Prescribed::Fixed {
                load,
                measure,
                effort,
            }
        }
        "to_effort" => {
            let (Some(load), Some(effort)) = (load, effort) else {
                return Err(corrupt(&"a to-effort set with no load or no effort"));
            };
            Prescribed::ToEffort {
                load,
                effort,
                predicted: target,
            }
        }
        "autoregulated" => {
            let (Some(measure), Some(effort)) = (target, effort) else {
                return Err(corrupt(
                    &"an autoregulated set with no measure or no effort",
                ));
            };
            Prescribed::Autoregulated { measure, effort }
        }
        other => return Err(corrupt(&format!("{other:?} is not a prescription variant"))),
    };

    Ok(PrescribedSet {
        prescription,
        rest_after: None,
        warmup: row.warmup != 0,
    })
}

/// The columns `rebuild` needs, lifted out of the anonymous record sqlx returns.
struct SetRow {
    variant: String,
    load_kind: Option<String>,
    load_grams: Option<i64>,
    target_low: Option<i64>,
    target_high: Option<i64>,
    effort: Option<String>,
    warmup: i64,
}
