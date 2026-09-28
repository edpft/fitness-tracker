//! The normalised layer for the gym sessions `garmin.activities` derives
//! (#172), and the reader that hands the translator each activity with its
//! sets.
//!
//! The sessions go in the source-independent gym tables beside Hevy's, the
//! spreadsheets' and Beyond The White Board's, told apart by `stream`, and only
//! this stream's rows are replaced. What a watch adds to them is two tables of
//! its own: `measured_gym_session` for the session's duration, its heart-rate
//! summary and the record its sets came from, and `measured_set` for the sets.
//!
//! **Its own set table rather than the shared one.** A `performed_set` hangs
//! off a `performed_exercise`, and a watch files sets under no exercise at all —
//! it sees a set start, counts reps and guesses the movement afterwards. Making
//! the shared table's exercise nullable would put a hole in every source's rows
//! to hold one source's shape.
//!
//! **Reading two landing tables is not one stream reaching into another**, for
//! the reason [`super::peloton_normalised`] gives: the extraction adapters stay
//! apart, and a derivation reads raw, of which both of these are Garmin's.

use application::{AccountReader, NormalisedEntityStore, StoreError};
use domain::{
    gym::{GuessedExercise, Load, MeasuredGymSession, MeasuredSet, RepsExercise},
    landing::{
        Endpoint, EventKind, EventProvenance, EventTime, FetchedAt, InvalidStream, LandedRecord,
        LandingRecord, LandingRecordId, LandingStream, Provenance, RawPayload, SourceRecordId,
    },
    normalised::{NormalisationRunId, WorkoutCount},
};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::garmin::{ActivityAccount, account::activities};

use super::{
    GarminActivityLandingStore, GarminExerciseSetLandingStore, corrupt, count_for_storage,
    count_from_storage, normalisation_run_for_storage, normalised::clear_stream, store_error,
};

/// Raw, read-only, for Garmin's gym sessions: every activity, each with the
/// sets that landed for it.
#[derive(Debug, Clone)]
pub struct GarminGymAccountReader {
    pool: SqlitePool,
    stream: LandingStream,
}

impl GarminGymAccountReader {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminActivityLandingStore::STREAM)?,
        })
    }
}

/// One row of either landing table, before it is a [`LandedRecord`].
struct Row {
    id: i64,
    endpoint: String,
    fetched_at: String,
    source_record_id: String,
    event_kind: String,
    event_time: Option<String>,
    payload: Vec<u8>,
}

impl Row {
    fn into_record(self, stream: &LandingStream) -> Result<LandedRecord, StoreError> {
        let occurred_at = self
            .event_time
            .as_deref()
            .map(EventTime::try_from)
            .transpose()
            .map_err(|error| corrupt(&error))?;

        let provenance = EventProvenance::new(
            Endpoint::try_from(self.endpoint.as_str()).map_err(|error| corrupt(&error))?,
            EventKind::try_from(self.event_kind.as_str()).map_err(|error| corrupt(&error))?,
            occurred_at,
        );

        let record = LandingRecord::land(
            stream.clone(),
            FetchedAt::try_from(self.fetched_at.as_str()).map_err(|error| corrupt(&error))?,
            SourceRecordId::try_from(self.source_record_id.as_str())
                .map_err(|error| corrupt(&error))?,
            provenance.into(),
            RawPayload::try_from(self.payload).map_err(|error| corrupt(&error))?,
        );

        Ok(LandedRecord::new(
            LandingRecordId::try_from(self.id).map_err(|error| corrupt(&error))?,
            record,
        ))
    }
}

impl AccountReader for GarminGymAccountReader {
    /// One activity and its sets.
    type Account = ActivityAccount;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn accounts(&self) -> Result<Vec<ActivityAccount>, StoreError> {
        let sets_stream = LandingStream::try_from(GarminExerciseSetLandingStore::STREAM)
            .map_err(|error| corrupt(&error))?;

        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM garmin_activity_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut landed = Vec::with_capacity(rows.len());
        for row in rows {
            landed.push(
                Row {
                    id: row.id,
                    endpoint: row.endpoint,
                    fetched_at: row.fetched_at,
                    source_record_id: row.source_record_id,
                    event_kind: row.event_kind,
                    event_time: row.event_time,
                    payload: row.payload,
                }
                .into_record(&self.stream)?,
            );
        }

        let set_rows = sqlx::query!(
            r#"
            SELECT id AS "id!: i64",
                   endpoint AS "endpoint!: String",
                   fetched_at AS "fetched_at!: String",
                   source_record_id AS "source_record_id!: String",
                   event_kind AS "event_kind!: String",
                   event_time AS "event_time: String",
                   payload AS "payload!: Vec<u8>"
            FROM garmin_exercise_set_landing
            ORDER BY id ASC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        let mut sets = Vec::with_capacity(set_rows.len());
        for row in set_rows {
            sets.push(
                Row {
                    id: row.id,
                    endpoint: row.endpoint,
                    fetched_at: row.fetched_at,
                    source_record_id: row.source_record_id,
                    event_kind: row.event_kind,
                    event_time: row.event_time,
                    payload: row.payload,
                }
                .into_record(&sets_stream)?,
            );
        }

        Ok(activities(landed, sets))
    }
}

/// Writes the gym sessions Garmin's activities derive.
#[derive(Debug, Clone)]
pub struct SqliteMeasuredGymSessionStore {
    pool: SqlitePool,
    stream: LandingStream,
}

impl SqliteMeasuredGymSessionStore {
    /// # Errors
    ///
    /// [`InvalidStream`] if the landing store's stream constant is not a stream
    /// name.
    pub fn new(pool: SqlitePool) -> Result<Self, InvalidStream> {
        Ok(Self {
            pool,
            stream: LandingStream::try_from(GarminActivityLandingStore::STREAM)?,
        })
    }
}

impl NormalisedEntityStore for SqliteMeasuredGymSessionStore {
    type Entity = MeasuredGymSession;

    fn stream(&self) -> &LandingStream {
        &self.stream
    }

    async fn replace(
        &self,
        run: NormalisationRunId,
        sessions: Vec<MeasuredGymSession>,
    ) -> Result<WorkoutCount, StoreError> {
        let run_id = normalisation_run_for_storage(run)?;
        let stream = self.stream.to_string();
        let written = sessions.len();

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|error| store_error(&error))?;
        clear_stream(&mut tx, &stream).await?;
        for session in &sessions {
            write_session(&mut tx, run_id, &stream, session).await?;
        }
        tx.commit().await.map_err(|error| store_error(&error))?;
        Ok(WorkoutCount::from(written))
    }

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

/// One session: the shared session and workout rows, what the watch measured,
/// and the sets.
///
/// **One workout per session**, as a log's is. A watch files one recording per
/// session and the operator never split one across two.
async fn write_session(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: i64,
    stream: &str,
    session: &MeasuredGymSession,
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
    let session_id = row.id;

    let started_at = session.started_at().instant().to_string();
    let zone = session.started_at().zone().id().to_owned();
    let source_record_id = session.source_record_id().as_str();
    let (endpoint, event_kind, event_time) = event_columns(session.provenance())?;

    let row = sqlx::query!(
        r#"
        INSERT INTO gym_workout (
            session, stream, landing_record_id, source_record_id, started_at_utc, zone,
            endpoint, event_kind, event_time, run_id
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        session_id,
        stream,
        landed_as,
        source_record_id,
        started_at,
        zone,
        endpoint,
        event_kind,
        event_time,
        run_id
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    let workout = row.id;

    let duration =
        i64::try_from(session.duration().as_seconds()).map_err(|_| StoreError::Corrupt {
            detail: "a duration larger than the store can hold".to_owned(),
        })?;
    let heart_rate = session.recorded().heart_rate();
    let average = heart_rate.map(|summary| i64::from(summary.average().as_u32()));
    let highest = heart_rate.map(|summary| i64::from(summary.highest().as_u32()));
    let sets_landed_as = session.sets_landed_as().map(LandingRecordId::as_i64);

    sqlx::query!(
        r#"
        INSERT INTO measured_gym_session (
            workout, duration_seconds, average_bpm, highest_bpm, sets_landing_record_id
        )
        VALUES (?, ?, ?, ?, ?)
        "#,
        workout,
        duration,
        average,
        highest,
        sets_landed_as
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;

    if let Some(sets) = session.recorded().sets() {
        for (position, set) in sets.iter().enumerate() {
            write_set(tx, workout, count_for_storage(position)?, set).await?;
        }
    }

    Ok(())
}

async fn write_set(
    tx: &mut Transaction<'_, Sqlite>,
    workout: i64,
    position: i64,
    set: &MeasuredSet,
) -> Result<(), StoreError> {
    let started_at = set.at.instant().to_string();
    let zone = set.at.zone().id().to_owned();
    let reps = i64::from(set.reps.as_u32());
    let (load_kind, load_grams) = match set.load {
        Some(Load::Absolute(mass)) => (
            Some("absolute"),
            Some(
                i64::try_from(mass.as_grams()).map_err(|_| StoreError::Corrupt {
                    detail: "a load larger than the store can hold".to_owned(),
                })?,
            ),
        ),
        Some(Load::Relative(delta)) => (Some("relative"), Some(delta.as_grams())),
        None => (None, None),
    };
    let guess = set.guess.movement().map(RepsExercise::as_str);
    // What the watch proposed for this set, and what was carried to it from the
    // run it sits in, are different claims and the row says which.
    let guess_from = match set.guess {
        GuessedExercise::Proposed(_) => Some("proposed"),
        GuessedExercise::FromItsRun(_) => Some("from-its-run"),
        GuessedExercise::Undetermined => None,
    };

    sqlx::query!(
        r#"
        INSERT INTO measured_set (
            workout, position, started_at_utc, zone, reps, load_kind, load_grams,
            guess, guess_from
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        workout,
        position,
        started_at,
        zone,
        reps,
        load_kind,
        load_grams,
        guess,
        guess_from
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(())
}

/// The provenance columns a landed record's event fills.
fn event_columns(provenance: &Provenance) -> Result<(String, String, Option<String>), StoreError> {
    let event = super::served_by_a_feed(provenance)?;
    Ok((
        event.endpoint().to_string(),
        event.kind().to_string(),
        event.occurred_at().map(|time| time.to_string()),
    ))
}

/// Every measured row one stream wrote, children first.
///
/// Called by [`super::normalised::clear_stream`], which owns the shared gym
/// tables: a stream's sets and its measured sessions have to go before the
/// workouts they point at.
pub(super) async fn clear_measured(
    tx: &mut Transaction<'_, Sqlite>,
    stream: &str,
) -> Result<(), StoreError> {
    sqlx::query!(
        "DELETE FROM measured_set WHERE workout IN (SELECT id FROM gym_workout WHERE stream = ?)",
        stream
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    sqlx::query!(
        "DELETE FROM measured_gym_session WHERE workout IN (SELECT id FROM gym_workout WHERE stream = ?)",
        stream
    )
    .execute(&mut **tx)
    .await
    .map_err(|error| store_error(&error))?;
    Ok(())
}

/// The movement a stored row names, and how it was arrived at.
///
/// # Errors
///
/// [`StoreError::Corrupt`] if the key is not one of ours, which is a row
/// written by a version whose vocabulary has since moved, or if the row says a
/// movement without saying where it came from.
pub fn guessed_from_row(
    key: Option<String>,
    from: Option<&str>,
) -> Result<GuessedExercise, StoreError> {
    let Some(key) = key else {
        return Ok(GuessedExercise::Undetermined);
    };
    let exercise = RepsExercise::try_from(key).map_err(|error| corrupt(&error))?;
    match from {
        Some("proposed") => Ok(GuessedExercise::Proposed(exercise)),
        Some("from-its-run") => Ok(GuessedExercise::FromItsRun(exercise)),
        other => Err(StoreError::Corrupt {
            detail: format!("{other:?} is not how a guess is arrived at"),
        }),
    }
}

/// How much raw the gym derivation reads: the activities and their sets.
///
/// Both, for the reason [`super::peloton_normalised::PelotonRawExtent`] counts
/// two tables: § 3.1 lets an entity compose the responses one source serves
/// about one thing, and a standing that counted only the activities would
/// report the layer up to date while a set walk was half done.
#[derive(Debug, Clone)]
pub struct GarminGymRawExtent {
    activities: GarminActivityLandingStore,
    sets: GarminExerciseSetLandingStore,
}

impl GarminGymRawExtent {
    pub const fn new(
        activities: GarminActivityLandingStore,
        sets: GarminExerciseSetLandingStore,
    ) -> Self {
        Self { activities, sets }
    }
}

impl application::RawExtent for GarminGymRawExtent {
    async fn records(&self) -> Result<domain::landing::RecordCount, StoreError> {
        let activities = application::LandingStore::count(&self.activities).await?;
        let sets = application::LandingStore::count(&self.sets).await?;
        Ok(domain::landing::RecordCount::from(
            activities.as_usize().saturating_add(sets.as_usize()),
        ))
    }
}
