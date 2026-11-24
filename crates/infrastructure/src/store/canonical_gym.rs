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
//! derivation is never mutated in place. A visit is replaced whole, and what is
//! replaced is either the whole layer or the visits on the days a change to the
//! normalised layer reaches (#610), each in one transaction.
//!
//! **A normalised writer records the days it changed, and empties what they
//! reach**, in its own transaction: a canonical session names the normalised
//! rows it came from, and a revision deletes those rows. So the layer always
//! holds what a whole derivation would give, except on the days a recorded
//! change reaches, where it holds nothing until the next derivation fills them.
//!
//! **And this is where everything downstream reads the gym record** (#350).
//! Three ports are served from these tables rather than from one source's:
//! whole sessions in a span, the session that answered a prescription, and when
//! each session happened. The fourth, [`application::ExerciseHistory`],
//! projects the same tables and lives in `history.rs` because what it returns is
//! a projection rather than the entity.

use std::collections::{BTreeMap, BTreeSet};

use application::{
    CanonicalGymSessionStore, CanonicalLayer, DeliveryReference, PerformedGymSessions,
    PerformedSessionLog, PrescribedWorkoutId, StoreError,
};
use domain::{
    canonical::{Attributed, NormalisedSessionId, Occurred, Reach, SessionCount, TakenFrom},
    gym::{
        self, Account, CanonicalExercise, CanonicalGymSession, CanonicalItem, CanonicalSet, Guess,
        GuessedExercise, Identified, Load, Performed, SetKind,
        edit::{Edit, Edits},
        exercise::{
            Description, DistanceExercise, DurationExercise, Implement, Movement, RepsExercise,
        },
        identity::{
            self, CanonicalExerciseId, CanonicalSessionId, CanonicalSetId, ExerciseIds, Issued,
            Placed, SessionIdentity, SessionIds, SourceSet,
        },
    },
    landing::{LandingStream, SourceRecordId},
    measure::{
        BeatsPerMinute, Distance, Duration, HeartRateSample, HeartRateSeries, HeartRateSummary,
        MeasuredHeartRate, Metres, PositiveDuration, RepCount,
    },
    normalised::{OperatorZone, SourceTerm, StartedAt},
    sequence::{AtLeastTwo, NonEmpty},
};
use jiff::{Timestamp, civil::Date};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

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

    pub(super) const fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

impl CanonicalGymSessionStore for SqliteCanonicalGymSessionStore {
    async fn replace(
        &self,
        sessions: Vec<CanonicalGymSession>,
        edits: &Edits,
    ) -> Result<SessionCount, StoreError> {
        let mut tx = super::writing(&self.pool)
            .await
            .map_err(|e| store_error(&e))?;
        clear(&mut tx).await?;
        let written = sessions.len();
        write_all(&mut tx, &sessions, edits).await?;
        tx.commit().await.map_err(|e| store_error(&e))?;
        Ok(SessionCount::from(written))
    }

    async fn reach(&self) -> Result<Reach, StoreError> {
        let days = super::canonical_changes::days(&self.pool, CanonicalLayer::Gym).await?;
        Ok(Reach::of(days, gym::REACHES))
    }

    async fn replace_within(
        &self,
        reach: &Reach,
        sessions: Vec<CanonicalGymSession>,
        edits: &Edits,
    ) -> Result<SessionCount, StoreError> {
        let mut tx = super::writing(&self.pool)
            .await
            .map_err(|e| store_error(&e))?;
        remove_on(&mut tx, reach.rebuilt()).await?;
        write_all(&mut tx, &sessions, edits).await?;
        super::canonical_changes::forget(&mut tx, CanonicalLayer::Gym, reach.changed()).await?;
        tx.commit().await.map_err(|e| store_error(&e))?;
        self.count().await
    }

    async fn edits(&self) -> Result<Vec<Edit>, StoreError> {
        super::canonical_gym_edits::all(&self.pool).await
    }

    async fn all(&self) -> Result<Vec<CanonicalGymSession>, StoreError> {
        self.reading().await?.all().await
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
    started_normalised_session: Option<i64>,
    ended_at_utc: Option<String>,
    ended_normalised_session: Option<i64>,
    average_bpm: Option<i64>,
    highest_bpm: Option<i64>,
    heart_rate_normalised_session: Option<i64>,
}

impl SqliteCanonicalGymSessionStore {
    /// One read of the layer, whole (#467).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn reading(&self) -> Result<CanonicalGymReading, StoreError> {
        Ok(CanonicalGymReading {
            snapshot: super::reading(&self.pool)
                .await
                .map_err(|e| store_error(&e))?,
        })
    }
}

/// One read of the canonical gym layer, every query in it seeing the layer as
/// it stood at the first (#467), whatever [`CanonicalGymSessionStore::replace`]
/// commits meanwhile.
///
/// **Why a read needs one.** `replace` rebuilds the layer in a transaction,
/// under fresh row ids, and a session is read in several queries: its row,
/// then its items, exercises, sets and heart rate. Outside a transaction a
/// session's row could come from before a rebuild and its sets from after,
/// and it would read as corrupt. Opening a page starts a run (#466), so a page
/// read while the layer is rebuilt is the ordinary case rather than a race.
///
/// A transaction that only reads, rolled back when this is dropped: SQLite's
/// write-ahead log lets `replace` commit while it is open.
pub struct CanonicalGymReading {
    snapshot: Transaction<'static, Sqlite>,
}

impl CanonicalGymReading {
    /// Every session, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read or holds something that is
    /// not a session.
    pub async fn all(&mut self) -> Result<Vec<CanonicalGymSession>, StoreError> {
        let rows = sqlx::query_as!(
            SessionRow,
            r#"
            SELECT id AS "id!", started_at_utc, zone, on_day,
                   started_normalised_session, ended_at_utc, ended_normalised_session,
                   average_bpm, highest_bpm, heart_rate_normalised_session
            FROM canonical_gym_session
            ORDER BY COALESCE(started_at_utc, on_day), id
            "#
        )
        .fetch_all(&mut *self.snapshot)
        .await
        .map_err(|e| store_error(&e))?;

        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            sessions.push(session_of(&mut self.snapshot, &row).await?);
        }
        Ok(sessions)
    }

    /// Every session on a day from `from` to `to`, both included.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read or holds something that is
    /// not a session.
    pub async fn between(
        &mut self,
        from: Date,
        to: Date,
    ) -> Result<Vec<CanonicalGymSession>, StoreError> {
        let mut sessions = Vec::new();
        for row in spine_between(&mut self.snapshot, from, to).await? {
            let session = session_of(&mut self.snapshot, &row).await?;
            let day = session.occurred().day();
            if day >= from && day <= to {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }
}

/// One whole session, its items and its heart rate.
async fn session_of(
    connection: &mut SqliteConnection,
    row: &SessionRow,
) -> Result<CanonicalGymSession, StoreError> {
    let occurred = Attributed::new(
        occurred(
            row.started_at_utc.as_deref(),
            row.zone.as_deref(),
            row.on_day.as_deref(),
        )?,
        taken_from(row.started_normalised_session)?,
    );
    let ended = row
        .ended_at_utc
        .as_deref()
        .map(|ended| {
            Ok::<_, StoreError>(Attributed::new(
                ended.parse::<Timestamp>().map_err(|e| corrupt(&e))?,
                taken_from(row.ended_normalised_session)?,
            ))
        })
        .transpose()?;
    let heart_rate = match (
        row.average_bpm,
        row.highest_bpm,
        row.heart_rate_normalised_session,
    ) {
        (Some(average), Some(highest), Some(session)) => {
            let summary = HeartRateSummary::new(beats(average)?, beats(highest)?);
            let series = read_series(connection, row.id).await?;
            Some(Attributed::new(
                MeasuredHeartRate::new(summary, series),
                normalised_session_from_storage(session)?,
            ))
        }
        _ => None,
    };
    let items = read_items(connection, row.id).await?;
    let accounts = read_accounts(connection, row.id).await?;
    Ok(CanonicalGymSession::new(
        occurred, items, heart_rate, ended, accounts,
    ))
}

/// Every spine row that could fall in the window, widest reading.
///
/// **Widened in SQL and narrowed in Rust.** Which day a session happened on
/// depends on the zone on its own row, so the exact comparison cannot be a
/// `WHERE` clause without assuming every session shares one offset. A day
/// either side covers every zone there is, and the precise filter runs through
/// [`Occurred::day`] — the same reading everything else uses.
async fn spine_between(
    connection: &mut SqliteConnection,
    from: Date,
    to: Date,
) -> Result<Vec<SessionRow>, StoreError> {
    let (lower, upper) = widened(from, to)?;
    sqlx::query_as!(
        SessionRow,
        r#"
            SELECT id AS "id!", started_at_utc, zone, on_day,
                   started_normalised_session, ended_at_utc, ended_normalised_session,
                   average_bpm, highest_bpm, heart_rate_normalised_session
            FROM canonical_gym_session
            WHERE COALESCE(started_at_utc, on_day) >= ?
              AND COALESCE(started_at_utc, on_day) < ?
            ORDER BY COALESCE(started_at_utc, on_day), id
            "#,
        lower,
        upper
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|e| store_error(&e))
}

/// A day either side of the window, as the strings the columns hold.
///
/// A day column is `YYYY-MM-DD` and an instant column `YYYY-MM-DDTHH:MM:SSZ`, so
/// one lexicographic comparison covers both: every instant on a day sorts after
/// that day's own string and before the next day's.
pub(super) fn widened(from: Date, to: Date) -> Result<(String, String), StoreError> {
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

/// Each session as History lists it (#817): its counts and stated averages in
/// one query, and none of its sets or its heart-rate series.
impl application::Activities for SqliteCanonicalGymSessionStore {
    async fn between(
        &self,
        from: Date,
        to: Date,
    ) -> Result<Vec<(Occurred, application::Activity)>, StoreError> {
        let (lower, upper) = widened(from, to)?;
        let rows = sqlx::query!(
            r#"
            SELECT s.started_at_utc, s.zone, s.on_day,
                   unixepoch(s.ended_at_utc) - unixepoch(s.started_at_utc) AS "duration_seconds: i64",
                   s.average_bpm,
                   (SELECT COUNT(*) FROM canonical_gym_exercise AS e
                     WHERE e.session = s.id) AS "exercises!: i64",
                   (SELECT COUNT(*) FROM canonical_gym_set AS t
                     WHERE t.session = s.id) AS "sets!: i64"
              FROM canonical_gym_session AS s
             WHERE COALESCE(s.started_at_utc, s.on_day) >= ?
               AND COALESCE(s.started_at_utc, s.on_day) < ?
             ORDER BY COALESCE(s.started_at_utc, s.on_day), s.id
            "#,
            lower,
            upper
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;
        let mut read = Vec::new();
        for row in rows {
            let occurred = occurred(
                row.started_at_utc.as_deref(),
                row.zone.as_deref(),
                row.on_day.as_deref(),
            )?;
            if !(from..=to).contains(&occurred.day()) {
                continue;
            }
            let length = row
                .duration_seconds
                .map(|seconds| {
                    PositiveDuration::from_seconds(u64::try_from(seconds).map_err(|e| corrupt(&e))?)
                        .map_err(|e| corrupt(&e))
                })
                .transpose()?;
            read.push((
                occurred,
                application::Activity::Gym {
                    length,
                    exercises: usize::try_from(row.exercises).map_err(|e| corrupt(&e))?,
                    sets: usize::try_from(row.sets).map_err(|e| corrupt(&e))?,
                    heart_rate: row.average_bpm.map(beats).transpose()?,
                    tested: Vec::new(),
                },
            ));
        }
        Ok(read)
    }

    async fn first(&self) -> Result<Option<Date>, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT started_at_utc, zone, on_day
              FROM canonical_gym_session
             ORDER BY COALESCE(started_at_utc, on_day), id
             LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| store_error(&e))?;
        row.map(|row| {
            occurred(
                row.started_at_utc.as_deref(),
                row.zone.as_deref(),
                row.on_day.as_deref(),
            )
            .map(|occurred| occurred.day())
        })
        .transpose()
    }
}

/// Whole canonical sessions, and the one that answered a prescription, each
/// read as one snapshot of the layer (#467).
impl PerformedGymSessions for SqliteCanonicalGymSessionStore {
    async fn between(&self, from: Date, to: Date) -> Result<Vec<CanonicalGymSession>, StoreError> {
        self.reading().await?.between(from, to).await
    }

    async fn fulfilling(
        &self,
        prescription: PrescribedWorkoutId,
    ) -> Result<Option<(DeliveryReference, CanonicalGymSession)>, StoreError> {
        self.reading().await?.fulfilling(prescription).await
    }
}

impl CanonicalGymReading {
    /// The session that answered a prescription, and the reference it was
    /// performed against.
    async fn fulfilling(
        &mut self,
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
        .fetch_optional(&mut *self.snapshot)
        .await
        .map_err(|e| store_error(&e))?;

        let Some(named) = named else {
            return Ok(None);
        };
        let reference =
            DeliveryReference::try_from(named.performed_against).map_err(|e| corrupt(&e))?;

        let Some(row) = standing_on(&mut self.snapshot, named.normalised_session).await? else {
            // A normalised session with no canonical one: the visit held no
            // exercise at all, or the layer has not been rebuilt since the
            // session landed. Neither is a fault here (§ 37), and reporting it
            // as "not performed" is what the caller already handles.
            return Ok(None);
        };
        Ok(Some((
            reference,
            session_of(&mut self.snapshot, &row).await?,
        )))
    }
}

/// The canonical session standing on one normalised session, where the layer
/// holds one.
///
/// **Every column that can attribute a field**, because any one of them may be
/// the only place a given normalised session is named: a sheet that contributed
/// nothing but one set's reps is named by that set's outcome and nowhere else.
/// The lowest id where two somehow matched — a normalised session belongs to
/// one visit, so that is a corrupt layer rather than a choice, and settling it
/// the same way every rebuild does beats picking whichever row SQLite offered
/// first.
async fn standing_on(
    connection: &mut SqliteConnection,
    normalised: i64,
) -> Result<Option<SessionRow>, StoreError> {
    sqlx::query_as!(
        SessionRow,
        r#"
            SELECT id AS "id!", started_at_utc, zone, on_day,
                   started_normalised_session, ended_at_utc, ended_normalised_session,
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
                 WHERE started_normalised_session = ?
                    OR ended_normalised_session = ?
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
        normalised,
        normalised
    )
    .fetch_optional(&mut *connection)
    .await
    .map_err(|e| store_error(&e))
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
/// re-deriving a normalised gym stream whole invalidates this whole layer,
/// because every field of it names a `gym_session` row that the re-derivation
/// deletes and rewrites under a new id. See [`super::normalised::clear_stream`].
/// A revision of some of a stream's sessions empties only what their days
/// reach, through [`changed`].
pub(super) async fn clear(tx: &mut Transaction<'_, Sqlite>) -> Result<(), StoreError> {
    for statement in [
        "DELETE FROM canonical_gym_set",
        "DELETE FROM canonical_gym_exercise",
        "DELETE FROM canonical_gym_item",
        "DELETE FROM canonical_gym_session_heart_rate",
        "DELETE FROM canonical_gym_session_account",
        "DELETE FROM canonical_gym_session",
    ] {
        sqlx::query(statement)
            .execute(&mut **tx)
            .await
            .map_err(|e| store_error(&e))?;
    }
    // A whole derivation follows, so no day is left to rebuild alone.
    super::canonical_changes::forget_all(tx, CanonicalLayer::Gym).await?;
    super::derivation_ledger::forget(tx, CanonicalLayer::Gym).await
}

/// Record that the normalised accounts on these days changed, and empty the
/// days that reaches (#610), in the transaction that changes them.
///
/// **Emptied now rather than at the next derivation**, because a canonical
/// session on one of those days may name a normalised row the caller is about
/// to delete, and the foreign key would refuse it.
pub(super) async fn changed(
    tx: &mut Transaction<'_, Sqlite>,
    days: BTreeSet<Date>,
) -> Result<(), StoreError> {
    if days.is_empty() {
        return Ok(());
    }
    super::canonical_changes::record(tx, CanonicalLayer::Gym, &days).await?;
    remove_on(tx, Reach::of(days, gym::REACHES).rebuilt()).await
}

/// Every canonical session on one of these days, in foreign-key order.
///
/// **The day is worked out here rather than in SQL**, because a session with
/// an instant is on the day its zone puts it on, which SQLite cannot resolve.
/// The spine is a few hundred rows.
async fn remove_on(
    tx: &mut Transaction<'_, Sqlite>,
    days: &BTreeSet<Date>,
) -> Result<(), StoreError> {
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", started_at_utc, zone, on_day FROM canonical_gym_session"#
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    let mut sessions = Vec::new();
    for row in rows {
        let when = occurred(
            row.started_at_utc.as_deref(),
            row.zone.as_deref(),
            row.on_day.as_deref(),
        )?;
        if days.contains(&when.day()) {
            sessions.push(row.id);
        }
    }
    if sessions.is_empty() {
        return Ok(());
    }
    let sessions = super::ids_json(sessions);
    for statement in [
        "DELETE FROM canonical_gym_set WHERE session IN (SELECT value FROM json_each(?))",
        "DELETE FROM canonical_gym_exercise WHERE session IN (SELECT value FROM json_each(?))",
        "DELETE FROM canonical_gym_item WHERE session IN (SELECT value FROM json_each(?))",
        "DELETE FROM canonical_gym_session_heart_rate \
         WHERE session IN (SELECT value FROM json_each(?))",
        "DELETE FROM canonical_gym_session_account \
         WHERE session IN (SELECT value FROM json_each(?))",
        "DELETE FROM canonical_gym_session WHERE id IN (SELECT value FROM json_each(?))",
    ] {
        sqlx::query(statement)
            .bind(&sessions)
            .execute(&mut **tx)
            .await
            .map_err(|e| store_error(&e))?;
    }
    Ok(())
}

/// Every session, each under the ids it keeps from the last build that held
/// its source sets, or new ones (#893), and with the operator's edits over it
/// (§ II.2).
///
/// **Where each source set was placed outlives the layer.** A normalised
/// writer empties these tables before the layer is rebuilt, so the last
/// placing cannot be read off the rows it replaces; `canonical_gym_source_set`
/// is never emptied, and is brought up to date here as each session is
/// written.
///
/// **Edits are applied here, after the ids are issued**, because an edit is
/// anchored to the id an entity keeps and which one it keeps is decided only
/// now. The edit names the id; it does not take part in deciding it.
async fn write_all(
    tx: &mut Transaction<'_, Sqlite>,
    sessions: &[CanonicalGymSession],
    edits: &Edits,
) -> Result<(), StoreError> {
    let placed = placed(tx).await?;
    let identities = identity::identify(sessions, &placed);
    for (session, identity) in sessions.iter().zip(identities) {
        let ids = issue(tx, identity).await?;
        let edited;
        let written = if edits.reach(&ids) {
            edited = edits.apply(session, &ids);
            &edited
        } else {
            session
        };
        let id = ids.session.as_i64();
        write_session(tx, id, written).await?;
        write_accounts(tx, id, written).await?;
        write_heart_rate(tx, id, written).await?;
        let mut exercises = ids.exercises.iter();
        for (position, item) in written.items().iter().enumerate() {
            write_item(tx, id, count_for_storage(position)?, item, &mut exercises).await?;
        }
        place(tx, written, &ids).await?;
    }
    Ok(())
}

/// Where every source set was last placed, leaving out any whose ids the
/// layer still holds: a session on a day this build does not rebuild keeps
/// its ids, and no rebuilt one can take them.
async fn placed(
    tx: &mut Transaction<'_, Sqlite>,
) -> Result<BTreeMap<SourceSet, Placed>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT p.stream AS "stream!", p.record AS "record!", p.term AS "term!",
               p.session AS "session!", p.exercise AS "exercise!",
               p.canonical_set AS "canonical_set!"
        FROM canonical_gym_source_set AS p
        WHERE NOT EXISTS (SELECT 1 FROM canonical_gym_session WHERE id = p.session)
          AND NOT EXISTS (SELECT 1 FROM canonical_gym_exercise WHERE id = p.exercise)
          AND NOT EXISTS (SELECT 1 FROM canonical_gym_set WHERE id = p.canonical_set)
        "#
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    let mut placed = BTreeMap::new();
    for row in rows {
        placed.insert(
            source_set(&row.stream, row.record, row.term)?,
            Placed {
                session: CanonicalSessionId::try_from(row.session).map_err(|e| corrupt(&e))?,
                exercise: CanonicalExerciseId::try_from(row.exercise).map_err(|e| corrupt(&e))?,
                set: CanonicalSetId::try_from(row.canonical_set).map_err(|e| corrupt(&e))?,
            },
        );
    }
    Ok(placed)
}

/// The ids a session's identity keeps, and new ones where it needs them.
async fn issue(
    tx: &mut Transaction<'_, Sqlite>,
    identity: SessionIdentity,
) -> Result<SessionIds, StoreError> {
    let session = match identity.session {
        Issued::Kept(id) => id,
        Issued::New => issued(
            sqlx::query_scalar!(
                r#"INSERT INTO canonical_gym_session_identity DEFAULT VALUES RETURNING id AS "id!""#
            )
            .fetch_one(&mut **tx)
            .await,
        )?,
    };
    let mut exercises = Vec::with_capacity(identity.exercises.len());
    for exercise in identity.exercises {
        let id = match exercise.exercise {
            Issued::Kept(id) => id,
            Issued::New => issued(
                sqlx::query_scalar!(
                    r#"INSERT INTO canonical_gym_exercise_identity DEFAULT VALUES RETURNING id AS "id!""#
                )
                .fetch_one(&mut **tx)
                .await,
            )?,
        };
        let mut sets = Vec::with_capacity(exercise.sets.len());
        for set in exercise.sets {
            sets.push(match set {
                Issued::Kept(id) => id,
                Issued::New => issued(
                    sqlx::query_scalar!(
                        r#"INSERT INTO canonical_gym_set_identity DEFAULT VALUES RETURNING id AS "id!""#
                    )
                    .fetch_one(&mut **tx)
                    .await,
                )?,
            });
        }
        exercises.push(ExerciseIds { exercise: id, sets });
    }
    Ok(SessionIds { session, exercises })
}

/// A newly issued id, as the type it identifies.
fn issued<I: TryFrom<i64, Error = identity::InvalidCanonicalId>>(
    inserted: Result<i64, sqlx::Error>,
) -> Result<I, StoreError> {
    I::try_from(inserted.map_err(|e| store_error(&e))?).map_err(|e| corrupt(&e))
}

/// Record where each of a session's source sets now is, in one statement.
async fn place(
    tx: &mut Transaction<'_, Sqlite>,
    session: &CanonicalGymSession,
    ids: &SessionIds,
) -> Result<(), StoreError> {
    let mut rows: Vec<(String, &str, &str, i64, i64, i64)> = Vec::new();
    for (exercise, held) in session.exercises().zip(&ids.exercises) {
        for (sources, set_id) in exercise.set_sources().into_iter().zip(&held.sets) {
            for source in sources {
                rows.push((
                    source.stream.to_string(),
                    source.record.as_str(),
                    source.term.as_str(),
                    ids.session.as_i64(),
                    held.exercise.as_i64(),
                    set_id.as_i64(),
                ));
            }
        }
    }
    if rows.is_empty() {
        return Ok(());
    }
    let rows = serde_json::to_string(&rows).map_err(|e| corrupt(&e))?;
    sqlx::query!(
        r#"
        INSERT INTO canonical_gym_source_set (stream, record, term, session, exercise, canonical_set)
        SELECT value ->> 0, value ->> 1, value ->> 2, value ->> 3, value ->> 4, value ->> 5
          FROM json_each(?)
        WHERE true
        ON CONFLICT (stream, record, term) DO UPDATE
           SET session = excluded.session,
               exercise = excluded.exercise,
               canonical_set = excluded.canonical_set
        "#,
        rows
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    Ok(())
}

async fn write_session(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    session: &CanonicalGymSession,
) -> Result<(), StoreError> {
    let (started_at_utc, zone, on_day) = match session.occurred() {
        Occurred::At(started_at) => (
            Some(started_at.instant().to_string()),
            Some(started_at.zone().id().to_owned()),
            None,
        ),
        Occurred::On(day) => (None, None, Some(day.to_string())),
    };
    let started_normalised_session = session
        .started()
        .normalised()
        .map(NormalisedSessionId::as_i64);
    let ended_at_utc = session.ended().map(|ended| ended.value().to_string());
    let ended_normalised_session = session
        .ended()
        .and_then(Attributed::normalised)
        .map(NormalisedSessionId::as_i64);
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
    sqlx::query!(
        r#"
        INSERT INTO canonical_gym_session (
            id, started_at_utc, zone, on_day, started_normalised_session,
            ended_at_utc, ended_normalised_session,
            average_bpm, highest_bpm, heart_rate_normalised_session
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        id,
        started_at_utc,
        zone,
        on_day,
        started_normalised_session,
        ended_at_utc,
        ended_normalised_session,
        average_bpm,
        highest_bpm,
        heart_rate_normalised_session
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    Ok(())
}

/// One account as it is written: its normalised session, its stream, its
/// start's instant, zone or day, and its end.
type AccountRow = (
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Each source's own start and end, in one statement.
async fn write_accounts(
    tx: &mut Transaction<'_, Sqlite>,
    session: i64,
    written: &CanonicalGymSession,
) -> Result<(), StoreError> {
    let mut rows: Vec<AccountRow> = Vec::new();
    for account in written.accounts() {
        let (started_at_utc, zone, on_day) = match &account.occurred {
            Occurred::At(started_at) => (
                Some(started_at.instant().to_string()),
                Some(started_at.zone().id().to_owned()),
                None,
            ),
            Occurred::On(day) => (None, None, Some(day.to_string())),
        };
        rows.push((
            account.normalised.as_i64(),
            account.stream.to_string(),
            started_at_utc,
            zone,
            on_day,
            account.ended.map(|ended| ended.to_string()),
        ));
    }
    if rows.is_empty() {
        return Ok(());
    }
    let rows = serde_json::to_string(&rows).map_err(|e| corrupt(&e))?;
    sqlx::query!(
        r#"
        INSERT INTO canonical_gym_session_account
            (session, normalised_session, stream, started_at_utc, zone, on_day, ended_at_utc)
        SELECT ?, value ->> 0, value ->> 1, value ->> 2, value ->> 3, value ->> 4, value ->> 5
          FROM json_each(?)
        "#,
        session,
        rows
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    Ok(())
}

/// Each source's own start and end, as the session holds them.
async fn read_accounts(
    connection: &mut SqliteConnection,
    session: i64,
) -> Result<Vec<Account>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT normalised_session AS "normalised!", stream, started_at_utc, zone, on_day,
               ended_at_utc
        FROM canonical_gym_session_account
        WHERE session = ?
        ORDER BY normalised_session
        "#,
        session
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|e| store_error(&e))?;
    rows.into_iter()
        .map(|row| {
            Ok(Account {
                stream: row.stream.parse().map_err(|e| corrupt(&e))?,
                normalised: normalised_session_from_storage(row.normalised)?,
                occurred: occurred(
                    row.started_at_utc.as_deref(),
                    row.zone.as_deref(),
                    row.on_day.as_deref(),
                )?,
                ended: row
                    .ended_at_utc
                    .map(|ended| ended.parse::<Timestamp>().map_err(|e| corrupt(&e)))
                    .transpose()?,
            })
        })
        .collect()
}

/// The watch's beats, as the canonical session holds them.
///
/// **One statement per session, the series handed over as a JSON array**, as
/// the canonical rides' are: a row per statement made rebuilding the
/// operator's 785,000 beats take over thirty seconds of every run that
/// reached the gym (#560).
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
    let mut rows = Vec::with_capacity(series.samples().count());
    for sample in series.samples().iter() {
        let at = i64::try_from(sample.at.as_seconds()).map_err(|e| corrupt(&e))?;
        rows.push(format!("[{at},{}]", sample.beats_per_minute.as_u32()));
    }
    let rows = format!("[{}]", rows.join(","));
    sqlx::query!(
        r#"
        INSERT INTO canonical_gym_session_heart_rate (session, at_seconds, beats_per_minute)
        SELECT ?, value ->> 0, value ->> 1
          FROM json_each(?)
        "#,
        session,
        rows,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| store_error(&e))?;
    Ok(())
}

async fn write_item(
    tx: &mut Transaction<'_, Sqlite>,
    session: i64,
    position: i64,
    item: &CanonicalItem,
    ids: &mut std::slice::Iter<'_, ExerciseIds>,
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
        let held = ids.next().ok_or_else(|| StoreError::Corrupt {
            detail: "a canonical exercise with no id issued for it".to_owned(),
        })?;
        let at = Position {
            session,
            item: position,
            exercise: count_for_storage(ordinal)?,
        };
        write_exercise(tx, at, held, exercise).await?;
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

/// Where an exercise sits: its session, its item and its place in the item.
#[derive(Clone, Copy)]
struct Position {
    session: i64,
    item: i64,
    exercise: i64,
}

async fn write_exercise(
    tx: &mut Transaction<'_, Sqlite>,
    at: Position,
    ids: &ExerciseIds,
    exercise: &CanonicalExercise,
) -> Result<(), StoreError> {
    let id = ids.exercise.as_i64();
    let set_ids = ids.sets.as_slice();
    let Position {
        session,
        item: item_position,
        exercise: position,
    } = at;
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
    let normalised_session = exercise.identified_by().map(NormalisedSessionId::as_i64);
    sqlx::query!(
        r#"
        INSERT INTO canonical_gym_exercise (
            id, session, item_position, position, measure,
            identified, exercise, movement, implement, identified_normalised_session
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        id,
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
            write_sets(tx, at, set_ids, sets, |reps| {
                Ok(Measure::Reps(i64::from(reps.as_u32())))
            })
            .await?;
        }
        CanonicalExercise::ForDuration { sets, .. } => {
            write_sets(tx, at, set_ids, sets, |duration| {
                Ok(Measure::Duration(
                    i64::try_from(duration.as_seconds()).map_err(|e| corrupt(&e))?,
                ))
            })
            .await?;
        }
        CanonicalExercise::ForDistance { sets, .. } => {
            write_sets(tx, at, set_ids, sets, |distance| {
                Ok(Measure::Distance(
                    i64::try_from(distance.metres.as_millimetres()).map_err(|e| corrupt(&e))?,
                ))
            })
            .await?;
        }
    }
    Ok(())
}

/// A completed set's measure, in the column its exercise counts it in.
enum Measure {
    Reps(i64),
    Duration(i64),
    Distance(i64),
}

/// Every set of one exercise, each under the id issued for it.
async fn write_sets<M: Copy + Sync>(
    tx: &mut Transaction<'_, Sqlite>,
    at: Position,
    set_ids: &[CanonicalSetId],
    sets: &NonEmpty<CanonicalSet<M>>,
    measure: impl Fn(M) -> Result<Measure, StoreError> + Send,
) -> Result<(), StoreError> {
    for (ordinal, set) in sets.iter().enumerate() {
        let measured = match set.outcome.value().completed().copied().flatten() {
            Some(amount) => Some(measure(amount)?),
            None => None,
        };
        let row = write_set(
            tx,
            SetAt {
                session: at.session,
                item_position: at.item,
                exercise_position: at.exercise,
                position: count_for_storage(ordinal)?,
                id: set_id(set_ids, ordinal)?,
            },
            set,
        )?;
        let row = match measured {
            Some(Measure::Reps(reps)) => row.reps(Some(reps)),
            Some(Measure::Duration(seconds)) => row.duration(Some(seconds)),
            Some(Measure::Distance(millimetres)) => row.distance(Some(millimetres)),
            None => row,
        };
        row.execute().await?;
    }
    Ok(())
}

/// The id issued for the `ordinal`th set of an exercise.
fn set_id(set_ids: &[CanonicalSetId], ordinal: usize) -> Result<i64, StoreError> {
    set_ids
        .get(ordinal)
        .map(|id| id.as_i64())
        .ok_or_else(|| StoreError::Corrupt {
            detail: "a canonical set with no id issued for it".to_owned(),
        })
}

/// Where a set sits, and the id it is written under.
#[derive(Clone, Copy)]
struct SetAt {
    session: i64,
    item_position: i64,
    exercise_position: i64,
    position: i64,
    id: i64,
}

/// A set row under construction: everything but the measure, which the caller
/// supplies because only it knows which column the measure belongs in.
struct SetRow<'tx, 'c> {
    tx: &'tx mut Transaction<'c, Sqlite>,
    id: i64,
    sources: String,
    session: i64,
    item_position: i64,
    exercise_position: i64,
    position: i64,
    outcome: &'static str,
    outcome_normalised_session: Option<i64>,
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
    at: SetAt,
    set: &CanonicalSet<M>,
) -> Result<SetRow<'tx, 'c>, StoreError> {
    let SetAt {
        session,
        item_position,
        exercise_position,
        position,
        id,
    } = at;
    let (load_kind, load_grams) = match set.load.as_ref().map(Attributed::copied) {
        Some(Load::Absolute(mass)) => (
            Some("absolute"),
            Some(i64::try_from(mass.as_grams()).unwrap_or(i64::MAX)),
        ),
        Some(Load::Relative(delta)) => (Some("relative"), Some(delta.as_grams())),
        None => (None, None),
    };
    Ok(SetRow {
        tx,
        id,
        sources: sources_for_storage(&set.sources)?,
        session,
        item_position,
        exercise_position,
        position,
        outcome: set.outcome.value().as_str(),
        outcome_normalised_session: set.outcome.normalised().map(NormalisedSessionId::as_i64),
        load_kind,
        load_grams,
        load_normalised_session: set
            .load
            .as_ref()
            .and_then(Attributed::normalised)
            .map(NormalisedSessionId::as_i64),
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
            .and_then(Attributed::normalised)
            .map(NormalisedSessionId::as_i64),
        rir: set
            .intensity
            .as_ref()
            .map(|rir| rir.copied().as_str().to_owned()),
        rir_normalised_session: set
            .intensity
            .as_ref()
            .and_then(Attributed::normalised)
            .map(NormalisedSessionId::as_i64),
        set_kind: set.kind.as_ref().map(|kind| match kind.copied() {
            SetKind::Working => "working",
            SetKind::Warmup => "warmup",
        }),
        set_kind_normalised_session: set
            .kind
            .as_ref()
            .and_then(Attributed::normalised)
            .map(NormalisedSessionId::as_i64),
        rest_after_seconds: set
            .rest_after
            .as_ref()
            .and_then(|rest| i64::try_from(rest.copied().as_seconds()).ok()),
        rest_after_normalised_session: set
            .rest_after
            .as_ref()
            .and_then(Attributed::normalised)
            .map(NormalisedSessionId::as_i64),
        reps: None,
        duration_seconds: None,
        distance_mm: None,
    })
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
                id, sources, session, item_position, exercise_position, position,
                outcome, reps, duration_seconds, distance_mm, outcome_normalised_session,
                load_kind, load_grams, load_normalised_session,
                began_at_utc, began_zone, began_normalised_session,
                rir, rir_normalised_session, set_kind, set_kind_normalised_session,
                rest_after_seconds, rest_after_normalised_session
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            self.id,
            self.sources,
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
    connection: &mut SqliteConnection,
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
    .fetch_all(&mut *connection)
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
    connection: &mut SqliteConnection,
    session: i64,
) -> Result<NonEmpty<CanonicalItem>, StoreError> {
    let items = sqlx::query!(
        r#"
        SELECT position AS "position!", is_superset AS "is_superset!"
        FROM canonical_gym_item WHERE session = ? ORDER BY position
        "#,
        session
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|e| store_error(&e))?;

    let mut built = Vec::with_capacity(items.len());
    for item in items {
        let exercises = read_exercises(connection, session, item.position).await?;
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
            CanonicalItem::exercise(one)
        });
    }
    NonEmpty::new(built).map_err(|e| corrupt(&e))
}

async fn read_exercises(
    connection: &mut SqliteConnection,
    session: i64,
    item_position: i64,
) -> Result<Vec<CanonicalExercise>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT position AS "position!", measure AS "measure!", identified AS "identified!",
               exercise, movement, implement, identified_normalised_session
        FROM canonical_gym_exercise
        WHERE session = ? AND item_position = ?
        ORDER BY position
        "#,
        session,
        item_position
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|e| store_error(&e))?;

    let mut built = Vec::with_capacity(rows.len());
    for row in rows {
        let identified_by = taken_from(row.identified_normalised_session)?;
        let sets = read_sets(connection, session, item_position, row.position).await?;
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
    pub(super) outcome_normalised_session: Option<i64>,
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
    pub(super) sources: BTreeSet<SourceSet>,
}

async fn read_sets(
    connection: &mut SqliteConnection,
    session: i64,
    item_position: i64,
    exercise_position: i64,
) -> Result<Vec<StoredSet>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT outcome AS "outcome!", reps, duration_seconds, distance_mm,
               outcome_normalised_session,
               load_kind, load_grams, load_normalised_session,
               began_at_utc, began_zone, began_normalised_session,
               rir, rir_normalised_session, set_kind, set_kind_normalised_session,
               rest_after_seconds, rest_after_normalised_session, sources AS "sources!"
        FROM canonical_gym_set
        WHERE session = ? AND item_position = ? AND exercise_position = ?
        ORDER BY position
        "#,
        session,
        item_position,
        exercise_position
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|e| store_error(&e))?;

    let mut sets = Vec::with_capacity(rows.len());
    for row in rows {
        sets.push(StoredSet {
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
            sources: sources_from_storage(&row.sources)?,
        });
    }
    Ok(sets)
}

/// A set's source sets, as the `sources` column holds them: a JSON array of
/// `[stream, record, term]`.
fn sources_from_storage(stored: &str) -> Result<BTreeSet<SourceSet>, StoreError> {
    let named: Vec<(String, String, String)> =
        serde_json::from_str(stored).map_err(|e| corrupt(&e))?;
    named
        .into_iter()
        .map(|(stream, record, term)| source_set(&stream, record, term))
        .collect()
}

/// One source set from its three stored parts.
pub(super) fn source_set(
    stream: &str,
    record: String,
    term: String,
) -> Result<SourceSet, StoreError> {
    Ok(SourceSet {
        stream: LandingStream::try_from(stream).map_err(|e| corrupt(&e))?,
        record: SourceRecordId::try_from(record).map_err(|e| corrupt(&e))?,
        term: SourceTerm::try_from(term).map_err(|e| corrupt(&e))?,
    })
}

fn sources_for_storage(sources: &BTreeSet<SourceSet>) -> Result<String, StoreError> {
    let named: Vec<(String, &str, &str)> = sources
        .iter()
        .map(|source| {
            (
                source.stream.to_string(),
                source.record.as_str(),
                source.term.as_str(),
            )
        })
        .collect();
    serde_json::to_string(&named).map_err(|e| corrupt(&e))
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
        (Some("absolute"), Some(grams), session) => Some(Attributed::new(
            Load::absolute(domain::measure::Kg::from_grams(
                u64::try_from(grams).map_err(|e| corrupt(&e))?,
            )),
            taken_from(session)?,
        )),
        (Some("relative"), Some(grams), session) => Some(Attributed::new(
            Load::relative(domain::gym::SignedKg::from_grams(grams)),
            taken_from(session)?,
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
        (Some(rir), session) => Some(Attributed::new(
            domain::gym::Rir::try_from(rir.to_owned()).map_err(|e| corrupt(&e))?,
            taken_from(session)?,
        )),
        (None, _) => None,
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
        outcome: Attributed::new(outcome, taken_from(row.outcome_normalised_session)?),
        load,
        began,
        intensity,
        kind,
        rest_after,
        sources: row.sources.clone(),
    })
}

/// Where a field the operator can edit was taken from: the normalised session
/// its column names, or his edit where it names none (§ II.2).
fn taken_from(session: Option<i64>) -> Result<TakenFrom, StoreError> {
    Ok(match session {
        Some(session) => TakenFrom::Normalised(normalised_session_from_storage(session)?),
        None => TakenFrom::Edit,
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

/// The canonical gym sessions, and the workouts naming what they performed
/// against (#809).
impl application::Input for SqliteCanonicalGymSessionStore {
    fn reads() -> application::Reads {
        application::Reads::of([application::Node::Canonical(
            application::CanonicalLayer::Gym,
        )])
        .and(super::tables(&[super::table::GYM_SESSION]))
    }
}

impl application::NotRaw for SqliteCanonicalGymSessionStore {}
