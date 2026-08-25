//! The normalised layer for `hevy.workouts`, and the reader raw is derived
//! from.
//!
//! Two adapters in one file because they are two halves of the same trip: one
//! reads the input, the other writes the derivation, and neither can do the
//! other's job. `HevyWorkoutLandingReader` has no `append` — that is what makes
//! "a derivation never writes to raw" a fact about the type rather than a
//! promise about the code.

use application::{LandingRecordReader, NormalisedWorkoutStore, StoreError};
use domain::{
    gym::{
        GymWorkout, Load, NormalisationRunId, PerformedExercise, Set, SetKind, WorkoutCount,
        WorkoutItem,
    },
    landing::{
        Endpoint, EventKind, EventProvenance, EventTime, FetchedAt, InvalidStream, LandedRecord,
        LandingRecord, LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{
    corrupt, count_for_storage, count_from_storage, normalisation_run_for_storage, store_error,
};

/// Raw, read-only, for Hevy workouts.
#[derive(Debug, Clone)]
pub struct HevyWorkoutLandingReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl HevyWorkoutLandingReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name. Taken from there rather than restated, so the reader and the
    /// writer cannot come to disagree about which table they are about.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(super::HevyWorkoutLandingStore::STREAM)?,
        })
    }
}

impl LandingRecordReader for HevyWorkoutLandingReader {
    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn records(&self) -> Result<Vec<LandedRecord>, StoreError> {
        // Oldest first, by the store's own sequence — which is the order the
        // source served them, because raw is append-only. Defined so a
        // derivation is reproducible, not because the derivation depends on
        // it: retraction is absorbing, and reversing this order is how that
        // gets tested.
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM hevy_workout_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let occurred_at = row
                .event_time
                .as_deref()
                .map(EventTime::try_from)
                .transpose()
                .map_err(|error| corrupt(&error))?;

            let provenance = EventProvenance::new(
                Endpoint::try_from(row.endpoint.as_str()).map_err(|error| corrupt(&error))?,
                EventKind::try_from(row.event_kind.as_str()).map_err(|error| corrupt(&error))?,
                occurred_at,
            );

            let record = LandingRecord::land(
                self.stream.clone(),
                FetchedAt::try_from(row.fetched_at.as_str()).map_err(|error| corrupt(&error))?,
                SourceRecordId::try_from(row.source_record_id.as_str())
                    .map_err(|error| corrupt(&error))?,
                provenance.into(),
                RawPayload::try_from(row.payload).map_err(|error| corrupt(&error))?,
            );

            records.push(LandedRecord::new(
                LandingRecordId::try_from(row.id).map_err(|error| corrupt(&error))?,
                record,
            ));
        }

        Ok(records)
    }
}

/// The normalised layer for Hevy workouts.
#[derive(Debug, Clone)]
pub struct SqliteGymWorkoutStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteGymWorkoutStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(super::HevyWorkoutLandingStore::STREAM)?,
        })
    }
}

impl NormalisedWorkoutStore for SqliteGymWorkoutStore {
    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        workouts: Vec<GymWorkout>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        // One transaction, and a replacement rather than an update. A
        // half-applied derivation is not a function of anything, and a
        // derivation that failed part-way must leave the previous one standing.
        sqlx::query!("DELETE FROM performed_set")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!("DELETE FROM performed_exercise")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!("DELETE FROM workout_item")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;
        sqlx::query!("DELETE FROM gym_workout")
            .execute(&mut *tx)
            .await
            .map_err(|error| store_error(&error))?;

        let written = workouts.len();
        for workout in &workouts {
            write_workout(&mut tx, run_id, workout).await?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let row = sqlx::query!(r#"SELECT count(*) AS "count!: i64" FROM gym_workout"#)
            .fetch_one(&self.pool)
            .await
            .map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(count_from_storage(Some(row.count))?))
    }
}

async fn write_workout(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    workout: &GymWorkout,
) -> Result<(), StoreError> {
    let landing_record_id = workout.landed_as().as_i64();
    let source_record_id = workout.source_record_id().as_str();
    let started_at = workout.started_at().instant().to_string();
    let zone = workout.started_at().zone().id();

    let domain::landing::Provenance::Event(event) = workout.provenance();
    let endpoint = event.endpoint().as_str();
    let event_kind = event.kind().as_str();
    let event_time = event.occurred_at().map(|at| at.to_string());

    // The session it was performed against, where the source named one. This
    // is what a prescription is joined to in order to know it was performed.
    let performed_against = workout
        .performed_against()
        .map(application::DeliveryReference::as_str);

    sqlx::query!(
        r#"
        INSERT INTO gym_workout (
            landing_record_id, source_record_id, started_at_utc, zone,
            endpoint, event_kind, event_time, run_id, performed_against
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        landing_record_id,
        source_record_id,
        started_at,
        zone,
        endpoint,
        event_kind,
        event_time,
        run_id,
        performed_against
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    for (position, item) in workout.items().iter().enumerate() {
        let position = count_for_storage(position)?;
        let is_superset = i64::from(matches!(item, WorkoutItem::Superset(_)));

        sqlx::query!(
            "INSERT INTO workout_item (workout, position, is_superset) VALUES (?, ?, ?)",
            landing_record_id,
            position,
            is_superset
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;

        for (member, exercise) in item.exercises().enumerate() {
            let member = count_for_storage(member)?;
            write_exercise(tx, landing_record_id, position, member, exercise).await?;
        }
    }

    Ok(())
}

async fn write_exercise(
    tx: &mut Transaction<'_, Sqlite>,
    workout: i64,
    item_position: i64,
    position: i64,
    exercise: &PerformedExercise,
) -> Result<(), StoreError> {
    let key = exercise.exercise_key();
    let measure = exercise.measure();

    sqlx::query!(
        r#"
        INSERT INTO performed_exercise (workout, item_position, position, exercise, measure)
        VALUES (?, ?, ?, ?, ?)
        "#,
        workout,
        item_position,
        position,
        key,
        measure
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    // Four arms rather than one, because a `Set<RepCount>` and a `Set<Duration>`
    // are different types. The measure columns below are the sum type projected
    // flat; which one is populated follows from the exercise, and is never read
    // back as "whichever column is filled".
    match exercise {
        PerformedExercise::ForReps { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let ordinal = count_for_storage(ordinal)?;
                let reps = set.outcome.completed().map(|reps| i64::from(reps.as_u32()));
                write_set(tx, Row::new(workout, item_position, position, ordinal, set))
                    .reps(reps)
                    .execute()
                    .await?;
            }
        }
        PerformedExercise::ForDuration { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let ordinal = count_for_storage(ordinal)?;
                let seconds = match set.outcome.completed() {
                    Some(duration) => Some(count_for_storage(
                        usize::try_from(duration.as_seconds()).map_err(|_| {
                            application::StoreError::Corrupt {
                                detail: "a duration larger than the store can hold".to_owned(),
                            }
                        })?,
                    )?),
                    None => None,
                };
                write_set(tx, Row::new(workout, item_position, position, ordinal, set))
                    .duration(seconds)
                    .execute()
                    .await?;
            }
        }
        PerformedExercise::ForDistance { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let ordinal = count_for_storage(ordinal)?;
                let millimetres = match set.outcome.completed() {
                    Some(distance) => Some(metres_for_storage(distance.metres)?),
                    None => None,
                };
                write_set(tx, Row::new(workout, item_position, position, ordinal, set))
                    .distance(millimetres)
                    .execute()
                    .await?;
            }
        }
    }

    Ok(())
}

/// The parts of a set row that do not depend on its measure.
struct Row {
    workout: i64,
    item_position: i64,
    exercise_position: i64,
    position: i64,
    load_kind: &'static str,
    load_grams: i64,
    /// `Performed<M>` projected. A failed attempt writes no measure at all,
    /// which is what the `0003` CHECK constraints hold.
    outcome: &'static str,
    rir: Option<String>,
    set_kind: &'static str,
    rest_after_seconds: Option<i64>,
}

impl Row {
    fn new<M>(
        workout: i64,
        item_position: i64,
        exercise_position: i64,
        position: i64,
        set: &Set<M>,
    ) -> Self {
        // SQLite stores signed integers, so the domain's unsigned mass narrows
        // here and nowhere else. A relative load is already signed.
        let (load_kind, load_grams) = match set.load {
            Load::Absolute(mass) => (
                "absolute",
                i64::try_from(mass.as_grams()).unwrap_or(i64::MAX),
            ),
            Load::Relative(delta) => ("relative", delta.as_grams()),
        };
        Self {
            workout,
            item_position,
            exercise_position,
            position,
            load_kind,
            load_grams,
            outcome: set.outcome.as_str(),
            rir: set.intensity.map(|rir| rir.as_str().to_owned()),
            set_kind: match set.kind {
                SetKind::Working => "working",
                SetKind::Warmup => "warmup",
            },
            rest_after_seconds: set
                .rest_after
                .and_then(|rest| i64::try_from(rest.as_seconds()).ok()),
        }
    }
}

/// A set row under construction, so the four measures share one `INSERT`.
struct SetWrite<'tx, 'conn> {
    tx: &'tx mut Transaction<'conn, Sqlite>,
    row: Row,
    reps: Option<i64>,
    duration: Option<i64>,
    distance: Option<i64>,
}

const fn write_set<'tx, 'conn>(
    tx: &'tx mut Transaction<'conn, Sqlite>,
    row: Row,
) -> SetWrite<'tx, 'conn> {
    SetWrite {
        tx,
        row,
        reps: None,
        duration: None,
        distance: None,
    }
}

impl SetWrite<'_, '_> {
    const fn reps(mut self, reps: Option<i64>) -> Self {
        self.reps = reps;
        self
    }

    const fn duration(mut self, duration: Option<i64>) -> Self {
        self.duration = duration;
        self
    }

    const fn distance(mut self, distance: Option<i64>) -> Self {
        self.distance = distance;
        self
    }

    async fn execute(self) -> Result<(), StoreError> {
        let row = self.row;
        sqlx::query!(
            r#"
            INSERT INTO performed_set (
                workout, item_position, exercise_position, position,
                load_kind, load_grams, outcome,
                reps, duration_seconds, distance_mm,
                rir, set_kind, rest_after_seconds
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            row.workout,
            row.item_position,
            row.exercise_position,
            row.position,
            row.load_kind,
            row.load_grams,
            row.outcome,
            self.reps,
            self.duration,
            self.distance,
            row.rir,
            row.set_kind,
            row.rest_after_seconds
        )
        .execute(&mut **self.tx)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }
}

/// A distance on its way into the store, checked rather than saturated.
fn metres_for_storage(metres: domain::gym::Metres) -> Result<i64, StoreError> {
    i64::try_from(metres.as_millimetres()).map_err(|_| StoreError::Corrupt {
        detail: "a distance larger than the store can hold".to_owned(),
    })
}
