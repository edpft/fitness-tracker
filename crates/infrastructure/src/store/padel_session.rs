//! Every padel session, read back for History (#772), and the operator's
//! changes of when one started and ended (#894).

use std::collections::BTreeMap;

use application::{
    Activities, Activity, Authors, Edited, Node, PlayedPadelSession, PlayedPadelSessions,
    StoreError,
};
use domain::{
    canonical::{
        Occurred,
        edit::{EditId, EditReason},
    },
    landing::SourceRecordId,
    measure::{
        BeatsPerMinute, Duration, HeartRateSample, HeartRateSeries, HeartRateSummary,
        MeasuredHeartRate, PositiveDuration,
    },
    normalised::{OperatorZone, StartedAt},
    padel::PadelEdit,
    sequence::NonEmpty,
};
use jiff::{Timestamp, civil::Date};
use sqlx::SqlitePool;

use super::{
    authoring::{Authoring, Writing},
    corrupt, store_error,
};

/// The read side of the padel sessions, and the writer of the changes to
/// them.
#[derive(Debug, Clone)]
pub struct SqlitePadelSessions {
    pool: SqlitePool,
}

/// What the changes to the padel sessions author.
struct PadelEdits;

impl Authors for PadelEdits {
    fn authors() -> std::collections::BTreeSet<Node> {
        [Node::Edits(Edited::Padel)].into()
    }
}

impl SqlitePadelSessions {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn writing(&self) -> Result<Writing<PadelEdits>, StoreError> {
        Authoring::new(self.pool.clone())
            .writing::<PadelEdits>()
            .await
    }

    /// Assert when the session the watch calls `session` started and how
    /// long it lasted, and why.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn change(
        &self,
        at: Timestamp,
        reason: &EditReason,
        session: &SourceRecordId,
        started: &StartedAt,
        duration: PositiveDuration,
    ) -> Result<EditId, StoreError> {
        let mut tx = self.writing().await?;
        let at = at.to_string();
        let reason = reason.as_str();
        let session = session.as_str();
        let started_at = started.instant().to_string();
        let zone = started.zone().id();
        let seconds = i64::try_from(duration.as_seconds()).map_err(|error| corrupt(&error))?;
        let id = sqlx::query_scalar!(
            r#"
            INSERT INTO padel_session_edit
                (session, asserted_at, reason, started_at_utc, zone, duration_seconds)
            VALUES (?, ?, ?, ?, ?, ?)
            RETURNING id AS "id!"
            "#,
            session,
            at,
            reason,
            started_at,
            zone,
            seconds
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|error| store_error(&error))?;
        tx.commit().await?;
        Ok(EditId::from(id))
    }

    /// Retract one change, which brings back the one before it, or the
    /// watch's account.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be written.
    pub async fn undo(&self, edit: EditId) -> Result<(), StoreError> {
        let mut tx = self.writing().await?;
        let id = edit.as_i64();
        sqlx::query!("DELETE FROM padel_session_edit WHERE id = ?", id)
            .execute(&mut **tx)
            .await
            .map_err(|error| store_error(&error))?;
        tx.commit().await
    }

    /// Every session that started from `lower` to before `upper`, as the
    /// strings the column holds, each with its change in force.
    async fn played(
        &self,
        lower: &str,
        upper: &str,
    ) -> Result<Vec<PlayedPadelSession>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT landing_record_id AS "landed!: i64",
                   source_record_id AS "session!: String",
                   started_at_utc AS "started_at!: String",
                   zone AS "zone!: String",
                   duration_seconds AS "duration!: i64",
                   average_bpm, highest_bpm
            FROM padel_session
            WHERE started_at_utc >= ? AND started_at_utc < ?
            ORDER BY started_at_utc, landing_record_id
            "#,
            lower,
            upper
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let mut edits = self.edits().await?;
        let mut read = Vec::with_capacity(rows.len());
        for row in rows {
            let session = SourceRecordId::try_from(row.session).map_err(|error| corrupt(&error))?;
            let changes = edits.remove(&session).unwrap_or_default();
            let heart_rate = match (row.average_bpm, row.highest_bpm) {
                (Some(average), Some(highest)) => {
                    let stated = HeartRateSummary::new(beats(average)?, beats(highest)?);
                    // The readings are read again only where a change moves
                    // the bounds they are read between.
                    let series = if changes.is_empty() {
                        None
                    } else {
                        self.series(row.landed).await?
                    };
                    Some(MeasuredHeartRate::new(stated, series))
                }
                _ => None,
            };
            read.push(
                PlayedPadelSession::recorded(
                    session,
                    started_at(&row.started_at, row.zone)?,
                    length(row.duration)?,
                    heart_rate,
                )
                .changed(changes),
            );
        }
        Ok(read)
    }

    /// Every change, by the session it changes.
    async fn edits(&self) -> Result<BTreeMap<SourceRecordId, Vec<PadelEdit>>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT id AS "id!", session, asserted_at, reason, started_at_utc, zone,
                   duration_seconds
            FROM padel_session_edit
            ORDER BY id
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let mut edits: BTreeMap<SourceRecordId, Vec<PadelEdit>> = BTreeMap::new();
        for row in rows {
            let session = SourceRecordId::try_from(row.session).map_err(|error| corrupt(&error))?;
            edits.entry(session).or_default().push(PadelEdit {
                id: EditId::from(row.id),
                asserted_at: row.asserted_at.parse().map_err(|error| corrupt(&error))?,
                reason: EditReason::try_from(row.reason).map_err(|error| corrupt(&error))?,
                started: started_at(&row.started_at_utc, row.zone)?,
                duration: length(row.duration_seconds)?,
            });
        }
        Ok(edits)
    }

    /// The readings the watch's recording of one session holds.
    async fn series(&self, landed: i64) -> Result<Option<HeartRateSeries>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT at_seconds AS "at_seconds!", beats_per_minute AS "beats_per_minute!"
            FROM padel_session_heart_rate
            WHERE landing_record_id = ?
            ORDER BY at_seconds
            "#,
            landed
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let mut samples = Vec::with_capacity(rows.len());
        for row in rows {
            samples.push(HeartRateSample {
                at: Duration::from_seconds(
                    u64::try_from(row.at_seconds).map_err(|error| corrupt(&error))?,
                ),
                beats_per_minute: beats(row.beats_per_minute)?,
            });
        }
        Ok(NonEmpty::new(samples).ok().map(HeartRateSeries::new))
    }

    /// Every session there is, each with its change in force: what the
    /// training done counts (#809).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable or holds something
    /// unreadable.
    pub async fn every(&self) -> Result<Vec<PlayedPadelSession>, StoreError> {
        // Every instant column sorts after the empty string and before `~`.
        self.played("", "~").await
    }
}

impl PlayedPadelSessions for SqlitePadelSessions {
    async fn between(&self, from: Date, to: Date) -> Result<Vec<PlayedPadelSession>, StoreError> {
        let (lower, upper) = super::canonical_gym::widened(from, to)?;
        Ok(self
            .played(&lower, &upper)
            .await?
            .into_iter()
            .filter(|played| (from..=to).contains(&played.started_at().wall_clock().date()))
            .collect())
    }
}

/// Each session as History lists it (#817), which is all of it but its highest
/// heart rate.
impl Activities for SqlitePadelSessions {
    async fn between(&self, from: Date, to: Date) -> Result<Vec<(Occurred, Activity)>, StoreError> {
        Ok(PlayedPadelSessions::between(self, from, to)
            .await?
            .into_iter()
            .map(|played| {
                (
                    Occurred::At(played.started_at().clone()),
                    Activity::Padel {
                        length: played.duration(),
                        heart_rate: played
                            .heart_rate()
                            .map(|heart_rate| heart_rate.stated().average()),
                    },
                )
            })
            .collect())
    }

    async fn first(&self) -> Result<Option<Date>, StoreError> {
        Ok(self
            .every()
            .await?
            .iter()
            .map(|played| played.started_at().wall_clock().date())
            .min())
    }
}

fn started_at(instant: &str, zone: String) -> Result<StartedAt, StoreError> {
    let instant: Timestamp = instant.parse().map_err(|error| corrupt(&error))?;
    let zone = OperatorZone::try_from(zone).map_err(|error| corrupt(&error))?;
    Ok(StartedAt::new(instant, zone))
}

fn length(seconds: i64) -> Result<PositiveDuration, StoreError> {
    let seconds = u64::try_from(seconds).map_err(|error| corrupt(&error))?;
    PositiveDuration::from_seconds(seconds).map_err(|error| corrupt(&error))
}

fn beats(bpm: i64) -> Result<BeatsPerMinute, StoreError> {
    u32::try_from(bpm)
        .map_err(|error| corrupt(&error))
        .and_then(|bpm| BeatsPerMinute::new(bpm).map_err(|error| corrupt(&error)))
}

/// `padel_session`, and the changes to it (#809, #894).
impl application::Input for SqlitePadelSessions {
    fn reads() -> application::Reads {
        super::tables(&[super::table::PADEL_SESSION])
            .and(application::Reads::of([Node::Edits(Edited::Padel)]))
    }
}

impl application::NotRaw for SqlitePadelSessions {}
