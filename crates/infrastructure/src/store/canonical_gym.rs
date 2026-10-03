//! The canonical layer for gym sessions: one row per visit, merged from
//! whichever normalised sessions recorded it (§ II.4).
//!
//! **Every field carries the normalised session it came from**, which is what the
//! `*_normalised_session` columns are. A canonical set's load and its reps may be two
//! different normalised sessions' — on 2020-10-09 the reps are
//! `the_beginner_prescription.xlsx`'s and the load is the watch's — so the
//! attribution is a column beside each value rather than one column on the row.
//!
//! **The heart rate is stored here rather than read back through the normalised
//! session that gave it.** § II.4 makes this the layer the analytical layer reads, and a
//! summary and a series reachable only through `measured_gym_session` would make
//! every reader of a canonical session reach past it into one source's tables.
//! What that costs is a second copy of the samples; what it buys is a canonical
//! layer that answers on its own.
//!
//! **Replacement rather than update**, as every derivation is: § II says a
//! derivation is never mutated in place, and the whole layer goes in one
//! transaction.
//!
//! **And this is where everything downstream reads the gym record** (#350).
//! Three ports are served from these tables rather than from one source's:
//! whole sessions in a span, the session that answered a prescription, and when
//! each session happened. The fourth, [`application::ExerciseHistory`],
//! projects the same tables and lives in `history.rs` because what it returns is
//! a projection rather than the entity.

use application::{
    CanonicalGymSessionStore, DeliveryReference, PerformedGymSessions, PerformedSessionLog,
    PrescribedWorkoutId, StoreError,
};
use domain::{
    canonical::{Attributed, NormalisedSessionId, Occurred, SessionCount},
    gym::{
        CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Guess,
        GuessedExercise, Identified, Load, MeasuredHeartRate, Performed, SetKind,
        exercise::{
            Description, DistanceExercise, DurationExercise, Implement, Movement, RepsExercise,
        },
    },
    measure::{
        BeatsPerMinute, Distance, Duration, HeartRateSample, HeartRateSeries, HeartRateSummary,
        Metres, PositiveDuration, RepCount,
    },
    normalised::{OperatorZone, StartedAt},
    sequence::{AtLeastTwo, NonEmpty},
};
use jiff::{Timestamp, civil::Date};
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{corrupt, count_for_storage, count_from_storage, store_error};

/// The canonical gym layer, in SQLite.
#[derive(Debug, Clone)]
pub struct SqliteCanonicalGymSessionStore {
    pool: SqlitePool,
}

impl SqliteCanonicalGymSessionStore {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl CanonicalGymSessionStore for SqliteCanonicalGymSessionStore {
    async fn replace(
        &self,
        sessions: Vec<CanonicalGymSession>,
    ) -> Result<SessionCount, StoreError> {
        let mut tx = self.pool.begin().await.map_err(|e| store_error(&e))?;
        clear(&mut tx).await?;
        let written = sessions.len();
        for session in &sessions {
            let id = write_session(&mut tx, session).await?;
            write_heart_rate(&mut tx, id, session).await?;
            for (position, item) in session.items().iter().enumerate() {
                write_item(&mut tx, id, count_for_storage(position)?, item).await?;
            }
        }
        tx.commit().await.map_err(|e| store_error(&e))?;
        Ok(SessionCount::from(written))
    }

    async fn all(&self) -> Result<Vec<CanonicalGymSession>, StoreError> {
        let rows = sqlx::query_as!(
            SessionRow,
            r#"
            SELECT id AS "id!", started_at_utc, zone, on_day,
                   duration_seconds, duration_normalised_session,
                   average_bpm, highest_bpm, heart_rate_normalised_session
            FROM canonical_gym_session
            ORDER BY COALESCE(started_at_utc, on_day), id
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;

        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            sessions.push(self.session_of(&row).await?);
        }
        Ok(sessions)
    }

    async fn count(&self) -> Result<SessionCount, StoreError> {
        let count =
            sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!" FROM canonical_gym_session"#)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| store_error(&e))?;
        Ok(SessionCount::from(count_from_storage(Some(count))?))
    }
}

/// One row of the layer's spine, as both the whole read and the span reads
/// select it.
struct SessionRow {
    id: i64,
    started_at_utc: Option<String>,
    zone: Option<String>,
    on_day: Option<String>,
    duration_seconds: Option<i64>,
    duration_normalised_session: Option<i64>,
    average_bpm: Option<i64>,
    highest_bpm: Option<i64>,
    heart_rate_normalised_session: Option<i64>,
}

impl SqliteCanonicalGymSessionStore {
    /// One whole session, its items and its heart rate.
    async fn session_of(&self, row: &SessionRow) -> Result<CanonicalGymSession, StoreError> {
        let occurred = occurred(
            row.started_at_utc.as_deref(),
            row.zone.as_deref(),
            row.on_day.as_deref(),
        )?;
        let duration = match (row.duration_seconds, row.duration_normalised_session) {
            (Some(seconds), Some(session)) => Some(Attributed::new(
                PositiveDuration::from_seconds(u64::try_from(seconds).map_err(|e| corrupt(&e))?)
                    .map_err(|e| corrupt(&e))?,
                normalised_session_from_storage(session)?,
            )),
            _ => None,
        };
        let heart_rate = match (
            row.average_bpm,
            row.highest_bpm,
            row.heart_rate_normalised_session,
        ) {
            (Some(average), Some(highest), Some(session)) => {
                let summary = HeartRateSummary::new(beats(average)?, beats(highest)?);
                let series = read_series(&self.pool, row.id).await?;
                Some(Attributed::new(
                    MeasuredHeartRate::new(summary, series),
                    normalised_session_from_storage(session)?,
                ))
            }
            _ => None,
        };
        let items = read_items(&self.pool, row.id).await?;
        Ok(CanonicalGymSession::new(
            occurred, items, heart_rate, duration,
        ))
    }

    /// Every spine row that could fall in the window, widest reading.
    ///
    /// **Widened in SQL and narrowed in Rust.** Which day a session happened on
    /// depends on the zone on its own row, so the exact comparison cannot be a
    /// `WHERE` clause without assuming every session shares one offset. A day
    /// either side covers every zone there is, and the precise filter runs
    /// through [`Occurred::day`] — the same reading everything else uses.
    async fn spine_between(&self, from: Date, to: Date) -> Result<Vec<SessionRow>, StoreError> {
        let (lower, upper) = widened(from, to)?;
        sqlx::query_as!(
            SessionRow,
            r#"
            SELECT id AS "id!", started_at_utc, zone, on_day,
                   duration_seconds, duration_normalised_session,
                   average_bpm, highest_bpm, heart_rate_normalised_session
            FROM canonical_gym_session
            WHERE COALESCE(started_at_utc, on_day) >= ?
              AND COALESCE(started_at_utc, on_day) < ?
            ORDER BY COALESCE(started_at_utc, on_day), id
            "#,
            lower,
            upper
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))
    }
}

/// A day either side of the window, as the strings the columns hold.
///
/// A day column is `YYYY-MM-DD` and an instant column `YYYY-MM-DDTHH:MM:SSZ`, so
/// one lexicographic comparison covers both: every instant on a day sorts after
/// that day's own string and before the next day's.
fn widened(from: Date, to: Date) -> Result<(String, String), StoreError> {
    let lower = from
        .checked_sub(jiff::Span::new().days(1))
        .map_err(|_| StoreError::Corrupt {
            detail: "a date before the calendar".to_owned(),
        })?
        .to_string();
    let upper = to
        .checked_add(jiff::Span::new().days(2))
        .map_err(|_| StoreError::Corrupt {
            detail: "a date beyond the calendar".to_owned(),
        })?
        .to_string();
    Ok((lower, upper))
}

/// Whole canonical sessions, and the one that answered a prescription.
impl PerformedGymSessions for SqliteCanonicalGymSessionStore {
    async fn between(&self, from: Date, to: Date) -> Result<Vec<CanonicalGymSession>, StoreError> {
        let mut sessions = Vec::new();
        for row in self.spine_between(from, to).await? {
            let session = self.session_of(&row).await?;
            let day = session.occurred().day();
            if day >= from && day <= to {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }

    async fn fulfilling(
        &self,
        prescription: PrescribedWorkoutId,
    ) -> Result<Option<(DeliveryReference, CanonicalGymSession)>, StoreError> {
        let id = prescription.as_i64();

        // **The normalised session names the prescription; the canonical one is
        // the visit.** Only the source a prescription is delivered to records
        // what a workout was performed against, so the reference is found on its
        // own rows exactly as it was before the canonical layer existed — and the
        // visit is then whichever canonical session stands on that normalised
        // session.
        //
        // Any delivery of this prescription will do: a reference a performance
        // names is a reference that was delivered, so which destination it went
        // to is not a question this has to answer.
        let named = sqlx::query!(
            r#"
            SELECT w.session AS "normalised_session!: i64",
                   w.performed_against AS "performed_against!: String"
            FROM gym_workout AS w
            JOIN prescription_delivery AS d ON d.reference = w.performed_against
            WHERE d.prescription = ? AND w.stream = 'hevy.workouts'
            ORDER BY w.started_at_utc ASC, w.landing_record_id ASC
            LIMIT 1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;

        let Some(named) = named else {
            return Ok(None);
        };
        let reference =
            DeliveryReference::try_from(named.performed_against).map_err(|e| corrupt(&e))?;

        let Some(row) = self.standing_on(named.normalised_session).await? else {
            // A normalised session with no canonical one: the visit held no
            // exercise at all, or the layer has not been rebuilt since the
            // session landed. Neither is a fault here (§ 37), and reporting it
            // as "not performed" is what the caller already handles.
            return Ok(None);
        };
        Ok(Some((reference, self.session_of(&row).await?)))
    }
}

impl SqliteCanonicalGymSessionStore {
    /// The canonical session standing on one normalised session, where the layer
    /// holds one.
    ///
    /// **Every column that can attribute a field**, because any one of them may
    /// be the only place a given normalised session is named: a sheet that
    /// contributed nothing but one set's reps is named by that set's outcome and
    /// nowhere else. The lowest id where two somehow matched — a normalised
    /// session belongs to one visit, so that is a corrupt layer rather than a
    /// choice, and settling it the same way every rebuild does beats picking
    /// whichever row SQLite offered first.
    async fn standing_on(&self, normalised: i64) -> Result<Option<SessionRow>, StoreError> {
        sqlx::query_as!(
            SessionRow,
            r#"
            SELECT id AS "id!", started_at_utc, zone, on_day,
                   duration_seconds, duration_normalised_session,
                   average_bpm, highest_bpm, heart_rate_normalised_session
            FROM canonical_gym_session
            WHERE id IN (
                SELECT session FROM canonical_gym_exercise
                 WHERE identified_normalised_session = ?
                UNION
                SELECT session FROM canonical_gym_set
                 WHERE outcome_normalised_session = ?
                    OR load_normalised_session = ?
                    OR began_normalised_session = ?
                    OR rir_normalised_session = ?
                    OR set_kind_normalised_session = ?
                    OR rest_after_normalised_session = ?
                UNION
                SELECT id FROM canonical_gym_session
                 WHERE duration_normalised_session = ?
                    OR heart_rate_normalised_session = ?
            )
            ORDER BY id
            LIMIT 1
            "#,
            normalised,
            normalised,
            normalised,
            normalised,
            normalised,
            normalised,
            normalised,
            normalised,
            normalised
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| store_error(&e))
    }
}

/// When the gym's sessions happened, at the grain the record knows.
///
/// **The spine alone**, because that is the whole question: a slot asks whether
/// the record holds a session to account for it, and reading every item and
/// every heart-rate sample of eleven years to answer it would be work with no
/// reader.
impl PerformedSessionLog for SqliteCanonicalGymSessionStore {
    async fn performed_between(&self, from: Date, to: Date) -> Result<Vec<Occurred>, StoreError> {
        let (lower, upper) = widened(from, to)?;
        let rows = sqlx::query!(
            r#"
            SELECT started_at_utc, zone, on_day
            FROM canonical_gym_session
            WHERE COALESCE(started_at_utc, on_day) >= ?
              AND COALESCE(started_at_utc, on_day) < ?
            ORDER BY COALESCE(started_at_utc, on_day), id
            "#,
            lower,
            upper
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;

        let mut happened = Vec::with_capacity(rows.len());
        for row in rows {
            let occurred = occurred(
                row.started_at_utc.as_deref(),
                row.zone.as_deref(),
                row.on_day.as_deref(),
            )?;
            let day = occurred.day();
            if day >= from && day <= to {
                happened.push(occurred);
            }
        }
        Ok(happened)
    }
}

/// Everything the layer holds, in foreign-key order.
///
/// **Called from two places**, and the second is the reason it is `pub(super)`:
/// re-deriving any normalised gym stream invalidates this whole layer, because
/// every field of it names a `gym_session` row that the re-derivation deletes and
/// rewrites under a new id. See [`super::normalised::clear_stream`].
pub(super) async fn clear(tx: &mut Transaction<'_, Sqlite>) -> Result<(), StoreError> {
    for statement in [
        "DELETE FROM canonical_gym_set",
        "DELETE FROM canonical_gym_exercise",
        "DELETE FROM canonical_gym_item",
        "DELETE FROM canonical_gym_session_heart_rate",
        "DELETE FROM canonical_gym_session",
    ] {
        sqlx::query(statement)
            .execute(&mut **tx)
            .await
            .map_err(|e| store_error(&e))?;
    }
    Ok(())
}

async fn write_session(
    tx: &mut Transaction<'_, Sqlite>,
    session: &CanonicalGymSession,
) -> Result<i64, StoreError> {
    let (started_at_utc, zone, on_day) = match session.occurred() {
        Occurred::At(started_at) => (
            Some(started_at.instant().to_string()),
            Some(started_at.zone().id().to_owned()),
            None,
        ),
        Occurred::On(day) => (None, None, Some(day.to_string())),
    };
    let (duration_seconds, duration_normalised_session) = match session.duration() {
        Some(duration) => (
            Some(i64::try_from(duration.copied().as_seconds()).map_err(|e| corrupt(&e))?),
            Some(duration.normalised_session().as_i64()),
        ),
        None => (None, None),
    };
    let (average_bpm, highest_bpm, heart_rate_normalised_session) =
        session
            .heart_rate()
            .map_or((None, None, None), |heart_rate| {
                let stated = heart_rate.value().stated();
                (
                    Some(i64::from(stated.average().as_u32())),
                    Some(i64::from(stated.highest().as_u32())),
                    Some(heart_rate.normalised_session().as_i64()),
                )
            });
    let id = sqlx::query!(
        r#"
        INSERT INTO canonical_gym_session (
            started_at_utc, zone, on_day, duration_seconds, duration_normalised_session,
            average_bpm, highest_bpm, heart_rate_normalised_session
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id AS "id!"
        "#,
        started_at_utc,
        zone,
        on_day,
        duration_seconds,
        duration_normalised_session,
        average_bpm,
        highest_bpm,
        heart_rate_normalised_session
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?
    .id;
    Ok(id)
}

async fn write_heart_rate(
    tx: &mut Transaction<'_, Sqlite>,
    session: i64,
    held: &CanonicalGymSession,
) -> Result<(), StoreError> {
    let Some(series) = held
        .heart_rate()
        .map(Attributed::value)
        .and_then(MeasuredHeartRate::series)
    else {
        return Ok(());
    };
    for sample in series.samples().iter() {
        let at = i64::try_from(sample.at.as_seconds()).map_err(|e| corrupt(&e))?;
        let bpm = i64::from(sample.beats_per_minute.as_u32());
        sqlx::query!(
            r#"
            INSERT INTO canonical_gym_session_heart_rate (session, at_seconds, beats_per_minute)
            VALUES (?, ?, ?)
            "#,
            session,
            at,
            bpm
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| store_error(&e))?;
    }
    Ok(())
}

async fn write_item(
    tx: &mut Transaction<'_, Sqlite>,
    session: i64,
    position: i64,
    item: &CanonicalItem,
) -> Result<(), StoreError> {
    let is_superset = i64::from(matches!(item, CanonicalItem::Superset(_)));
    sqlx::query!(
        "INSERT INTO canonical_gym_item (session, position, is_superset) VALUES (?, ?, ?)",
        session,
        position,
        is_superset
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    for (ordinal, exercise) in item.exercises().enumerate() {
        write_exercise(tx, session, position, count_for_storage(ordinal)?, exercise).await?;
    }
    Ok(())
}

/// The four columns an exercise's identity goes in: how it was arrived at, and
/// the exercise or the movement and implement it reached.
fn identity_columns(
    identified: Identified,
) -> (
    &'static str,
    Option<&'static str>,
    Option<&'static str>,
    Option<&'static str>,
) {
    match identified {
        Identified::Recorded(exercise) => ("recorded", Some(exercise.as_str()), None, None),
        Identified::Guessed(guessed) => {
            let how = match guessed {
                GuessedExercise::Proposed(_) => "proposed",
                GuessedExercise::FromItsRun(_) => "from-its-run",
                GuessedExercise::Undetermined => "undetermined",
            };
            match guessed.guess() {
                Some(Guess::Exercise(exercise)) => (how, Some(exercise.as_str()), None, None),
                Some(Guess::Description(description)) => (
                    how,
                    None,
                    Some(description.movement().as_str()),
                    description.implement().map(Implement::as_str),
                ),
                None => (how, None, None, None),
            }
        }
    }
}

async fn write_exercise(
    tx: &mut Transaction<'_, Sqlite>,
    session: i64,
    item_position: i64,
    position: i64,
    exercise: &CanonicalExercise,
) -> Result<(), StoreError> {
    let measure = exercise.measure();
    let (identified, named, movement, implement) = match exercise {
        CanonicalExercise::ForReps { identified, .. } => identity_columns(identified.copied()),
        CanonicalExercise::ForDuration { exercise, .. } => {
            ("recorded", Some(exercise.copied().as_str()), None, None)
        }
        CanonicalExercise::ForDistance { exercise, .. } => {
            ("recorded", Some(exercise.copied().as_str()), None, None)
        }
    };
    let normalised_session = exercise.identified_by().as_i64();
    sqlx::query!(
        r#"
        INSERT INTO canonical_gym_exercise (
            session, item_position, position, measure,
            identified, exercise, movement, implement, identified_normalised_session
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        session,
        item_position,
        position,
        measure,
        identified,
        named,
        movement,
        implement,
        normalised_session
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;

    match exercise {
        CanonicalExercise::ForReps { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let reps = set
                    .outcome
                    .value()
                    .completed()
                    .and_then(|reps| reps.map(|reps| i64::from(reps.as_u32())));
                write_set(
                    tx,
                    session,
                    item_position,
                    position,
                    count_for_storage(ordinal)?,
                    set,
                )
                .reps(reps)
                .execute()
                .await?;
            }
        }
        CanonicalExercise::ForDuration { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let seconds = match set.outcome.value().completed().copied().flatten() {
                    Some(duration) => {
                        Some(i64::try_from(duration.as_seconds()).map_err(|e| corrupt(&e))?)
                    }
                    None => None,
                };
                write_set(
                    tx,
                    session,
                    item_position,
                    position,
                    count_for_storage(ordinal)?,
                    set,
                )
                .duration(seconds)
                .execute()
                .await?;
            }
        }
        CanonicalExercise::ForDistance { sets, .. } => {
            for (ordinal, set) in sets.iter().enumerate() {
                let millimetres = match set.outcome.value().completed().copied().flatten() {
                    Some(distance) => Some(
                        i64::try_from(distance.metres.as_millimetres()).map_err(|e| corrupt(&e))?,
                    ),
                    None => None,
                };
                write_set(
                    tx,
                    session,
                    item_position,
                    position,
                    count_for_storage(ordinal)?,
                    set,
                )
                .distance(millimetres)
                .execute()
                .await?;
            }
        }
    }
    Ok(())
}

/// A set row under construction: everything but the measure, which the caller
/// supplies because only it knows which column the measure belongs in.
struct SetRow<'tx, 'c> {
    tx: &'tx mut Transaction<'c, Sqlite>,
    session: i64,
    item_position: i64,
    exercise_position: i64,
    position: i64,
    outcome: &'static str,
    outcome_normalised_session: i64,
    load_kind: Option<&'static str>,
    load_grams: Option<i64>,
    load_normalised_session: Option<i64>,
    began_at_utc: Option<String>,
    began_zone: Option<String>,
    began_normalised_session: Option<i64>,
    rir: Option<String>,
    rir_normalised_session: Option<i64>,
    set_kind: Option<&'static str>,
    set_kind_normalised_session: Option<i64>,
    rest_after_seconds: Option<i64>,
    rest_after_normalised_session: Option<i64>,
    reps: Option<i64>,
    duration_seconds: Option<i64>,
    distance_mm: Option<i64>,
}

fn write_set<'tx, 'c, M>(
    tx: &'tx mut Transaction<'c, Sqlite>,
    session: i64,
    item_position: i64,
    exercise_position: i64,
    position: i64,
    set: &CanonicalSet<M>,
) -> SetRow<'tx, 'c> {
    let (load_kind, load_grams) = match set.load.as_ref().map(Attributed::copied) {
        Some(Load::Absolute(mass)) => (
            Some("absolute"),
            Some(i64::try_from(mass.as_grams()).unwrap_or(i64::MAX)),
        ),
        Some(Load::Relative(delta)) => (Some("relative"), Some(delta.as_grams())),
        None => (None, None),
    };
    SetRow {
        tx,
        session,
        item_position,
        exercise_position,
        position,
        outcome: set.outcome.value().as_str(),
        outcome_normalised_session: set.outcome.normalised_session().as_i64(),
        load_kind,
        load_grams,
        load_normalised_session: set
            .load
            .as_ref()
            .map(|load| load.normalised_session().as_i64()),
        began_at_utc: set
            .began
            .as_ref()
            .map(|began| began.value().instant().to_string()),
        began_zone: set
            .began
            .as_ref()
            .map(|began| began.value().zone().id().to_owned()),
        began_normalised_session: set
            .began
            .as_ref()
            .map(|began| began.normalised_session().as_i64()),
        rir: set
            .intensity
            .as_ref()
            .map(|rir| rir.copied().as_str().to_owned()),
        rir_normalised_session: set
            .intensity
            .as_ref()
            .map(|rir| rir.normalised_session().as_i64()),
        set_kind: set.kind.as_ref().map(|kind| match kind.copied() {
            SetKind::Working => "working",
            SetKind::Warmup => "warmup",
        }),
        set_kind_normalised_session: set
            .kind
            .as_ref()
            .map(|kind| kind.normalised_session().as_i64()),
        rest_after_seconds: set
            .rest_after
            .as_ref()
            .and_then(|rest| i64::try_from(rest.copied().as_seconds()).ok()),
        rest_after_normalised_session: set
            .rest_after
            .as_ref()
            .map(|rest| rest.normalised_session().as_i64()),
        reps: None,
        duration_seconds: None,
        distance_mm: None,
    }
}

impl SetRow<'_, '_> {
    const fn reps(mut self, reps: Option<i64>) -> Self {
        self.reps = reps;
        self
    }

    const fn duration(mut self, seconds: Option<i64>) -> Self {
        self.duration_seconds = seconds;
        self
    }

    const fn distance(mut self, millimetres: Option<i64>) -> Self {
        self.distance_mm = millimetres;
        self
    }

    async fn execute(self) -> Result<(), StoreError> {
        sqlx::query!(
            r#"
            INSERT INTO canonical_gym_set (
                session, item_position, exercise_position, position,
                outcome, reps, duration_seconds, distance_mm, outcome_normalised_session,
                load_kind, load_grams, load_normalised_session,
                began_at_utc, began_zone, began_normalised_session,
                rir, rir_normalised_session, set_kind, set_kind_normalised_session,
                rest_after_seconds, rest_after_normalised_session
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            self.session,
            self.item_position,
            self.exercise_position,
            self.position,
            self.outcome,
            self.reps,
            self.duration_seconds,
            self.distance_mm,
            self.outcome_normalised_session,
            self.load_kind,
            self.load_grams,
            self.load_normalised_session,
            self.began_at_utc,
            self.began_zone,
            self.began_normalised_session,
            self.rir,
            self.rir_normalised_session,
            self.set_kind,
            self.set_kind_normalised_session,
            self.rest_after_seconds,
            self.rest_after_normalised_session
        )
        .execute(&mut **self.tx)
        .await
        .map_err(|error| store_error(&error))?;
        Ok(())
    }
}

pub(super) fn normalised_session_from_storage(id: i64) -> Result<NormalisedSessionId, StoreError> {
    NormalisedSessionId::try_from(id).map_err(|e| corrupt(&e))
}

pub(super) fn beats(value: i64) -> Result<BeatsPerMinute, StoreError> {
    BeatsPerMinute::new(u32::try_from(value).map_err(|e| corrupt(&e))?).map_err(|e| corrupt(&e))
}

pub(super) fn occurred(
    started_at_utc: Option<&str>,
    zone: Option<&str>,
    on_day: Option<&str>,
) -> Result<Occurred, StoreError> {
    match (started_at_utc, zone, on_day) {
        (Some(instant), Some(zone), _) => {
            let instant: Timestamp = instant.parse().map_err(|e| corrupt(&e))?;
            let zone = OperatorZone::try_from(zone.to_owned()).map_err(|e| corrupt(&e))?;
            Ok(Occurred::At(StartedAt::new(instant, zone)))
        }
        (_, _, Some(day)) => {
            let day: Date = day.parse().map_err(|e| corrupt(&e))?;
            Ok(Occurred::On(day))
        }
        _ => Err(StoreError::Corrupt {
            detail: "a canonical gym session with neither an instant nor a day".to_owned(),
        }),
    }
}

async fn read_series(
    pool: &SqlitePool,
    session: i64,
) -> Result<Option<HeartRateSeries>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT at_seconds AS "at_seconds!", beats_per_minute AS "beats_per_minute!"
        FROM canonical_gym_session_heart_rate
        WHERE session = ?
        ORDER BY at_seconds
        "#,
        session
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;

    let mut samples = Vec::with_capacity(rows.len());
    for row in rows {
        samples.push(HeartRateSample {
            at: Duration::from_seconds(u64::try_from(row.at_seconds).map_err(|e| corrupt(&e))?),
            beats_per_minute: beats(row.beats_per_minute)?,
        });
    }
    Ok(NonEmpty::new(samples).ok().map(HeartRateSeries::new))
}

async fn read_items(
    pool: &SqlitePool,
    session: i64,
) -> Result<NonEmpty<CanonicalItem>, StoreError> {
    let items = sqlx::query!(
        r#"
        SELECT position AS "position!", is_superset AS "is_superset!"
        FROM canonical_gym_item WHERE session = ? ORDER BY position
        "#,
        session
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;

    let mut built = Vec::with_capacity(items.len());
    for item in items {
        let exercises = read_exercises(pool, session, item.position).await?;
        built.push(if item.is_superset == 1 {
            CanonicalItem::Superset(Box::new(
                AtLeastTwo::new(exercises).map_err(|e| corrupt(&e))?,
            ))
        } else {
            let one = exercises
                .into_iter()
                .next()
                .ok_or_else(|| StoreError::Corrupt {
                    detail: "a canonical item with no exercise".to_owned(),
                })?;
            CanonicalItem::Exercise(one)
        });
    }
    NonEmpty::new(built).map_err(|e| corrupt(&e))
}

async fn read_exercises(
    pool: &SqlitePool,
    session: i64,
    item_position: i64,
) -> Result<Vec<CanonicalExercise>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT position AS "position!", measure AS "measure!", identified AS "identified!",
               exercise, movement, implement, identified_normalised_session AS "identified_normalised_session!"
        FROM canonical_gym_exercise
        WHERE session = ? AND item_position = ?
        ORDER BY position
        "#,
        session,
        item_position
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;

    let mut built = Vec::with_capacity(rows.len());
    for row in rows {
        let identified_by = normalised_session_from_storage(row.identified_normalised_session)?;
        let sets = read_sets(pool, session, item_position, row.position).await?;
        built.push(match row.measure.as_str() {
            "reps" => {
                let identified = identified_from_storage(
                    &row.identified,
                    row.exercise.as_deref(),
                    row.movement.as_deref(),
                    row.implement.as_deref(),
                )?;
                CanonicalExercise::ForReps {
                    identified: Attributed::new(identified, identified_by),
                    sets: NonEmpty::new(reps_sets(sets)?).map_err(|e| corrupt(&e))?,
                }
            }
            "duration" => CanonicalExercise::ForDuration {
                exercise: Attributed::new(
                    DurationExercise::try_from(named(row.exercise.as_deref())?.to_owned())
                        .map_err(|e| corrupt(&e))?,
                    identified_by,
                ),
                sets: NonEmpty::new(duration_sets(sets)?).map_err(|e| corrupt(&e))?,
            },
            "distance" => CanonicalExercise::ForDistance {
                exercise: Attributed::new(
                    DistanceExercise::try_from(named(row.exercise.as_deref())?.to_owned())
                        .map_err(|e| corrupt(&e))?,
                    identified_by,
                ),
                sets: NonEmpty::new(distance_sets(sets)?).map_err(|e| corrupt(&e))?,
            },
            other => {
                return Err(StoreError::Corrupt {
                    detail: format!("{other} is not a measure"),
                });
            }
        });
    }
    Ok(built)
}

fn named(exercise: Option<&str>) -> Result<&str, StoreError> {
    exercise.ok_or_else(|| StoreError::Corrupt {
        detail: "an exercise counted in duration or distance that names none".to_owned(),
    })
}

pub(super) fn identified_from_storage(
    how: &str,
    exercise: Option<&str>,
    movement: Option<&str>,
    implement: Option<&str>,
) -> Result<Identified, StoreError> {
    if how == "recorded" {
        let exercise =
            RepsExercise::try_from(named(exercise)?.to_owned()).map_err(|e| corrupt(&e))?;
        return Ok(Identified::Recorded(exercise));
    }
    if how == "undetermined" {
        return Ok(Identified::Guessed(GuessedExercise::Undetermined));
    }
    let guess = match (exercise, movement) {
        (Some(exercise), _) => {
            Guess::Exercise(RepsExercise::try_from(exercise.to_owned()).map_err(|e| corrupt(&e))?)
        }
        (None, Some(movement)) => {
            let movement = Movement::try_from(movement.to_owned()).map_err(|e| corrupt(&e))?;
            let described = Description::of(movement);
            Guess::Description(match implement {
                Some(implement) => described.loaded_with(
                    Implement::try_from(implement.to_owned()).map_err(|e| corrupt(&e))?,
                ),
                None => described,
            })
        }
        (None, None) => {
            return Err(StoreError::Corrupt {
                detail: format!("a {how} guess that names nothing"),
            });
        }
    };
    Ok(Identified::Guessed(match how {
        "proposed" => GuessedExercise::Proposed(guess),
        "from-its-run" => GuessedExercise::FromItsRun(guess),
        other => {
            return Err(StoreError::Corrupt {
                detail: format!("{other} is not how an exercise is identified"),
            });
        }
    }))
}

/// A set row, before its measure is read into the type its exercise fixes.
///
/// **Shared with the normalised reader**, which fills every attribution slot
/// with the one session it is reading. A canonical set is assembled by one
/// piece of code whether it comes from these tables or from a projection of
/// the layer below, so the two cannot drift.
pub(super) struct StoredSet {
    pub(super) outcome: String,
    pub(super) reps: Option<i64>,
    pub(super) duration_seconds: Option<i64>,
    pub(super) distance_mm: Option<i64>,
    pub(super) outcome_normalised_session: i64,
    pub(super) load_kind: Option<String>,
    pub(super) load_grams: Option<i64>,
    pub(super) load_normalised_session: Option<i64>,
    pub(super) began_at_utc: Option<String>,
    pub(super) began_zone: Option<String>,
    pub(super) began_normalised_session: Option<i64>,
    pub(super) rir: Option<String>,
    pub(super) rir_normalised_session: Option<i64>,
    pub(super) set_kind: Option<String>,
    pub(super) set_kind_normalised_session: Option<i64>,
    pub(super) rest_after_seconds: Option<i64>,
    pub(super) rest_after_normalised_session: Option<i64>,
}

async fn read_sets(
    pool: &SqlitePool,
    session: i64,
    item_position: i64,
    exercise_position: i64,
) -> Result<Vec<StoredSet>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT outcome AS "outcome!", reps, duration_seconds, distance_mm,
               outcome_normalised_session AS "outcome_normalised_session!",
               load_kind, load_grams, load_normalised_session,
               began_at_utc, began_zone, began_normalised_session,
               rir, rir_normalised_session, set_kind, set_kind_normalised_session,
               rest_after_seconds, rest_after_normalised_session
        FROM canonical_gym_set
        WHERE session = ? AND item_position = ? AND exercise_position = ?
        ORDER BY position
        "#,
        session,
        item_position,
        exercise_position
    )
    .fetch_all(pool)
    .await
    .map_err(|e| store_error(&e))?;

    Ok(rows
        .into_iter()
        .map(|row| StoredSet {
            outcome: row.outcome,
            reps: row.reps,
            duration_seconds: row.duration_seconds,
            distance_mm: row.distance_mm,
            outcome_normalised_session: row.outcome_normalised_session,
            load_kind: row.load_kind,
            load_grams: row.load_grams,
            load_normalised_session: row.load_normalised_session,
            began_at_utc: row.began_at_utc,
            began_zone: row.began_zone,
            began_normalised_session: row.began_normalised_session,
            rir: row.rir,
            rir_normalised_session: row.rir_normalised_session,
            set_kind: row.set_kind,
            set_kind_normalised_session: row.set_kind_normalised_session,
            rest_after_seconds: row.rest_after_seconds,
            rest_after_normalised_session: row.rest_after_normalised_session,
        })
        .collect())
}

/// Everything about a set but its measure, which its exercise's variant fixes.
fn common<M>(
    row: &StoredSet,
    outcome: Performed<Option<M>>,
) -> Result<CanonicalSet<M>, StoreError> {
    let load = match (
        row.load_kind.as_deref(),
        row.load_grams,
        row.load_normalised_session,
    ) {
        (Some("absolute"), Some(grams), Some(session)) => Some(Attributed::new(
            Load::absolute(domain::measure::Kg::from_grams(
                u64::try_from(grams).map_err(|e| corrupt(&e))?,
            )),
            normalised_session_from_storage(session)?,
        )),
        (Some("relative"), Some(grams), Some(session)) => Some(Attributed::new(
            Load::relative(domain::gym::SignedKg::from_grams(grams)),
            normalised_session_from_storage(session)?,
        )),
        (None, None, None) => None,
        _ => {
            return Err(StoreError::Corrupt {
                detail: "a canonical set's load is half written".to_owned(),
            });
        }
    };
    let began = match (
        row.began_at_utc.as_deref(),
        row.began_zone.as_deref(),
        row.began_normalised_session,
    ) {
        (Some(instant), Some(zone), Some(session)) => {
            let instant: Timestamp = instant.parse().map_err(|e| corrupt(&e))?;
            let zone = OperatorZone::try_from(zone.to_owned()).map_err(|e| corrupt(&e))?;
            Some(Attributed::new(
                StartedAt::new(instant, zone),
                normalised_session_from_storage(session)?,
            ))
        }
        _ => None,
    };
    let intensity = match (row.rir.as_deref(), row.rir_normalised_session) {
        (Some(rir), Some(session)) => Some(Attributed::new(
            domain::gym::Rir::try_from(rir.to_owned()).map_err(|e| corrupt(&e))?,
            normalised_session_from_storage(session)?,
        )),
        _ => None,
    };
    let kind = match (row.set_kind.as_deref(), row.set_kind_normalised_session) {
        (Some("working"), Some(session)) => Some(Attributed::new(
            SetKind::Working,
            normalised_session_from_storage(session)?,
        )),
        (Some("warmup"), Some(session)) => Some(Attributed::new(
            SetKind::Warmup,
            normalised_session_from_storage(session)?,
        )),
        _ => None,
    };
    let rest_after = match (row.rest_after_seconds, row.rest_after_normalised_session) {
        (Some(seconds), Some(session)) => Some(Attributed::new(
            Duration::from_seconds(u64::try_from(seconds).map_err(|e| corrupt(&e))?),
            normalised_session_from_storage(session)?,
        )),
        _ => None,
    };
    Ok(CanonicalSet {
        outcome: Attributed::new(
            outcome,
            normalised_session_from_storage(row.outcome_normalised_session)?,
        ),
        load,
        began,
        intensity,
        kind,
        rest_after,
    })
}

fn outcome_of<M>(stored: &str, measure: Option<M>) -> Result<Performed<Option<M>>, StoreError> {
    match stored {
        "completed" => Ok(Performed::Completed(measure)),
        "failed" => Ok(Performed::Failed),
        other => Err(StoreError::Corrupt {
            detail: format!("{other} is not an outcome"),
        }),
    }
}

pub(super) fn reps_sets(rows: Vec<StoredSet>) -> Result<Vec<CanonicalSet<RepCount>>, StoreError> {
    let mut sets = Vec::with_capacity(rows.len());
    for row in rows {
        let reps = match row.reps {
            Some(reps) => Some(
                RepCount::new(u32::try_from(reps).map_err(|e| corrupt(&e))?)
                    .map_err(|e| corrupt(&e))?,
            ),
            None => None,
        };
        let outcome = outcome_of(&row.outcome, reps)?;
        sets.push(common(&row, outcome)?);
    }
    Ok(sets)
}

pub(super) fn duration_sets(
    rows: Vec<StoredSet>,
) -> Result<Vec<CanonicalSet<Duration>>, StoreError> {
    let mut sets = Vec::with_capacity(rows.len());
    for row in rows {
        let seconds = match row.duration_seconds {
            Some(seconds) => Some(Duration::from_seconds(
                u64::try_from(seconds).map_err(|e| corrupt(&e))?,
            )),
            None => None,
        };
        let outcome = outcome_of(&row.outcome, seconds)?;
        sets.push(common(&row, outcome)?);
    }
    Ok(sets)
}

pub(super) fn distance_sets(
    rows: Vec<StoredSet>,
) -> Result<Vec<CanonicalSet<Distance>>, StoreError> {
    let mut sets = Vec::with_capacity(rows.len());
    for row in rows {
        let distance = match row.distance_mm {
            Some(millimetres) => Some(Distance {
                metres: Metres::from_millimetres(
                    u64::try_from(millimetres).map_err(|e| corrupt(&e))?,
                ),
            }),
            None => None,
        };
        let outcome = outcome_of(&row.outcome, distance)?;
        sets.push(common(&row, outcome)?);
    }
    Ok(sets)
}
