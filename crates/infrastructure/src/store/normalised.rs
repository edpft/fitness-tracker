//! The normalised layer for `hevy.workouts`, and the account raw is derived
//! from.
//!
//! Two adapters in one file because they are two halves of the same trip: one
//! reads the input, the other writes the derivation, and neither can do the
//! other's job. `HevySessionAccountReader` has no `append` — that is what makes
//! "a derivation never writes to raw" a fact about the type rather than a
//! promise about the code.
//!
//! **The reader reads one table and groups what it finds into sessions.** Hevy
//! serves a workout whole, so unlike its Peloton counterpart there is no second
//! endpoint to join — but the operator split single sessions across several
//! routines, and § 3.1 composes those into one entity.
//!
//! **The grouping rule is not here.** Which of Hevy's records belong to one
//! session lives in [`crate::hevy::sessions`]. This adapter reads rows and hands
//! them over. What it keeps is the half only it can do: reaching a store, so
//! that what the translator receives is whole and cannot go back for more.

use application::{AccountReader, NormalisedEntityStore, StoreError};
use domain::{
    gym::{
        GymWorkout, Load, ManualSet, PerformedExercise, PerformedGymSession, Set, SetKind,
        WorkoutItem,
    },
    landing::{
        Endpoint, EventKind, EventProvenance, EventTime, FetchedAt, InvalidStream, LandedRecord,
        LandingRecord, LandingRecordId, LandingStream, RawPayload, SourceRecordId,
    },
    normalised::{NormalisationRunId, WorkoutCount},
};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::hevy::{SessionAccount, group};

use super::{
    corrupt, count_for_storage, count_from_storage, normalisation_run_for_storage, store_error,
};

/// Raw, read-only, for Hevy gym sessions.
#[derive(Debug, Clone)]
pub struct HevySessionAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl HevySessionAccountReader {
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

impl AccountReader for HevySessionAccountReader {
    /// One session: the workouts it was split across, in the order performed.
    type Account = SessionAccount;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self) -> Result<Vec<SessionAccount>, StoreError> {
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

        Ok(group(records))
    }
}

/// The normalised layer for Hevy gym sessions.
#[derive(Debug, Clone)]
pub struct SqliteGymSessionStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteGymSessionStore {
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

impl NormalisedEntityStore for SqliteGymSessionStore {
    type Entity = PerformedGymSession;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        sessions: Vec<PerformedGymSession>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let stream = self.stream.to_string();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;

        // One transaction, and a replacement rather than an update. A
        // half-applied derivation is not a function of anything, and a
        // derivation that failed part-way must leave the previous one standing.
        // Only this stream's rows: the tables hold every source's sessions.
        clear_stream(&mut tx, &stream).await?;

        let written = sessions.len();
        for session in &sessions {
            write_session(&mut tx, run_id, &stream, session).await?;
        }

        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

    /// **Sessions, not workouts.** What this store holds is entities, and the
    /// count is what a `status` line reports as derived — so counting the parts
    /// would report 167 where the layer holds 146.
    async fn count(&self) -> Result<WorkoutCount, StoreError> {
        let stream = self.stream.to_string();
        let row = sqlx::query!(
            r#"SELECT count(*) AS "count!: i64" FROM gym_session WHERE stream = ?"#,
            stream
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(count_from_storage(Some(row.count))?))
    }
}

/// Every row one stream wrote into the gym tables, children first.
///
/// Shared with the spreadsheets' store, because the tables are every source's
/// (constitution § 3.1) and a derivation replaces only its own stream.
pub(super) async fn clear_stream(
    tx: &mut Transaction<'_, Sqlite>,
    stream: &str,
) -> Result<(), StoreError> {
    sqlx::query!(
        "DELETE FROM performed_set WHERE workout IN (SELECT id FROM gym_workout WHERE stream = ?)",
        stream
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    sqlx::query!(
        "DELETE FROM performed_exercise WHERE workout IN (SELECT id FROM gym_workout WHERE stream = ?)",
        stream
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    sqlx::query!(
        "DELETE FROM workout_item WHERE workout IN (SELECT id FROM gym_workout WHERE stream = ?)",
        stream
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    sqlx::query!("DELETE FROM gym_workout WHERE stream = ?", stream)
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    // Last, because the workouts above point at it.
    sqlx::query!("DELETE FROM gym_session WHERE stream = ?", stream)
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
    Ok(())
}

/// One session and every workout it was performed as.
async fn write_session(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    stream: &str,
    session: &PerformedGymSession,
) -> Result<(), StoreError> {
    let landed_as = session.landed_as().as_i64();

    let row = sqlx::query!(
        r#"
        INSERT INTO gym_session (stream, landing_record_id, run_id) VALUES (?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        stream,
        landed_as,
        run_id
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    for workout in session.workouts().iter() {
        write_workout(tx, run_id, stream, workout, row.id).await?;
    }

    Ok(())
}

async fn write_workout(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    stream: &str,
    workout: &GymWorkout,
    session: i64,
) -> Result<(), StoreError> {
    let landing_record_id = workout.landed_as().as_i64();
    let source_record_id = workout.source_record_id().as_str();
    let started_at = workout.started_at().instant().to_string();
    let zone = workout.started_at().zone().id();

    let event = super::served_by_a_feed(workout.provenance())?;
    let endpoint = event.endpoint().as_str();
    let event_kind = event.kind().as_str();
    let event_time = event.occurred_at().map(|at| at.to_string());

    // The session it was performed against, where the source named one. This
    // is what a prescription is joined to in order to know it was performed.
    let performed_against = workout
        .performed_against()
        .map(application::DeliveryReference::as_str);

    let row = sqlx::query!(
        r#"
        INSERT INTO gym_workout (
            stream, landing_record_id, source_record_id, started_at_utc, zone,
            endpoint, event_kind, event_time, run_id, performed_against, session
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        stream,
        landing_record_id,
        source_record_id,
        started_at,
        zone,
        endpoint,
        event_kind,
        event_time,
        run_id,
        performed_against,
        session
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    let workout_id = row.id;

    for (position, item) in workout.items().iter().enumerate() {
        let position = count_for_storage(position)?;
        let is_superset = i64::from(matches!(item, WorkoutItem::Superset(_)));

        sqlx::query!(
            "INSERT INTO workout_item (workout, position, is_superset) VALUES (?, ?, ?)",
            workout_id,
            position,
            is_superset
        )
        .execute(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;

        for (member, exercise) in item.exercises().enumerate() {
            let member = count_for_storage(member)?;
            write_exercise(tx, workout_id, position, member, exercise).await?;
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
pub(super) struct Row {
    workout: i64,
    item_position: i64,
    exercise_position: i64,
    position: i64,
    /// Both `None` only where a sheet did not record the load. Hevy always
    /// does.
    load_kind: Option<&'static str>,
    load_grams: Option<i64>,
    /// `Performed<M>` projected. A failed attempt writes no measure at all,
    /// which is what the `0003` CHECK constraints hold.
    outcome: &'static str,
    rir: Option<String>,
    set_kind: &'static str,
    rest_after_seconds: Option<i64>,
    /// The sheet and cell a spreadsheet recorded the set in. `None` for a
    /// feed.
    sheet: Option<String>,
    cell: Option<String>,
}

/// A load as the two columns it is stored in.
///
/// SQLite stores signed integers, so the domain's unsigned mass narrows here
/// and nowhere else. A relative load is already signed.
fn load_columns(load: Load) -> (&'static str, i64) {
    match load {
        Load::Absolute(mass) => (
            "absolute",
            i64::try_from(mass.as_grams()).unwrap_or(i64::MAX),
        ),
        Load::Relative(delta) => ("relative", delta.as_grams()),
    }
}

const fn set_kind_column(kind: SetKind) -> &'static str {
    match kind {
        SetKind::Working => "working",
        SetKind::Warmup => "warmup",
    }
}

impl Row {
    fn new<M>(
        workout: i64,
        item_position: i64,
        exercise_position: i64,
        position: i64,
        set: &Set<M>,
    ) -> Self {
        let (load_kind, load_grams) = load_columns(set.load);
        Self {
            workout,
            item_position,
            exercise_position,
            position,
            load_kind: Some(load_kind),
            load_grams: Some(load_grams),
            outcome: set.outcome.as_str(),
            rir: set.intensity.map(|rir| rir.as_str().to_owned()),
            set_kind: set_kind_column(set.kind),
            rest_after_seconds: set
                .rest_after
                .and_then(|rest| i64::try_from(rest.as_seconds()).ok()),
            sheet: None,
            cell: None,
        }
    }

    /// A set a sheet recorded, which may not say what the load was.
    ///
    /// A sheet records no supersets, so every item is one exercise and the
    /// exercise is the first member of its item.
    pub(super) fn manual<M>(
        workout: i64,
        item_position: i64,
        position: i64,
        set: &ManualSet<M>,
    ) -> Self {
        let load = set.load.map(load_columns);
        Self {
            workout,
            item_position,
            exercise_position: 0,
            position,
            load_kind: load.map(|(kind, _)| kind),
            load_grams: load.map(|(_, grams)| grams),
            outcome: set.outcome.as_str(),
            rir: set.intensity.map(|rir| rir.as_str().to_owned()),
            set_kind: set_kind_column(set.kind),
            rest_after_seconds: set
                .rest_after
                .and_then(|rest| i64::try_from(rest.as_seconds()).ok()),
            sheet: Some(set.written_in.sheet.to_string()),
            cell: Some(set.written_in.cell.to_string()),
        }
    }
}

/// A set row under construction, so the four measures share one `INSERT`.
pub(super) struct SetWrite<'tx, 'conn> {
    tx: &'tx mut Transaction<'conn, Sqlite>,
    row: Row,
    reps: Option<i64>,
    duration: Option<i64>,
    distance: Option<i64>,
}

pub(super) const fn write_set<'tx, 'conn>(
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
    pub(super) const fn reps(mut self, reps: Option<i64>) -> Self {
        self.reps = reps;
        self
    }

    pub(super) const fn duration(mut self, duration: Option<i64>) -> Self {
        self.duration = duration;
        self
    }

    const fn distance(mut self, distance: Option<i64>) -> Self {
        self.distance = distance;
        self
    }

    pub(super) async fn execute(self) -> Result<(), StoreError> {
        let row = self.row;
        sqlx::query!(
            r#"
            INSERT INTO performed_set (
                workout, item_position, exercise_position, position,
                load_kind, load_grams, outcome,
                reps, duration_seconds, distance_mm,
                rir, set_kind, rest_after_seconds, sheet, cell
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            row.rest_after_seconds,
            row.sheet,
            row.cell
        )
        .execute(&mut **self.tx)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }
}

/// A distance on its way into the store, checked rather than saturated.
fn metres_for_storage(metres: domain::measure::Metres) -> Result<i64, StoreError> {
    i64::try_from(metres.as_millimetres()).map_err(|_| StoreError::Corrupt {
        detail: "a distance larger than the store can hold".to_owned(),
    })
}
