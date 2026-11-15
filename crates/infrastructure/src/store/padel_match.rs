//! Every padel match, read back for History (#772).

use application::{Activities, Activity, PlayedPadelMatch, PlayedPadelMatches, StoreError};
use domain::{
    canonical::Occurred,
    measure::{BeatsPerMinute, HeartRateSummary, PositiveDuration},
    normalised::{OperatorZone, StartedAt},
};
use jiff::civil::Date;
use sqlx::SqlitePool;

use super::{corrupt, store_error};

/// The read side of the padel matches.
#[derive(Debug, Clone)]
pub struct SqlitePadelMatches {
    pool: SqlitePool,
}

impl SqlitePadelMatches {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl PlayedPadelMatches for SqlitePadelMatches {
    async fn between(&self, from: Date, to: Date) -> Result<Vec<PlayedPadelMatch>, StoreError> {
        let (lower, upper) = super::canonical_gym::widened(from, to)?;
        let rows = sqlx::query!(
            r#"
            SELECT started_at_utc AS "started_at!: String",
                   zone AS "zone!: String",
                   duration_seconds AS "duration!: i64",
                   average_bpm, highest_bpm
            FROM padel_match
            WHERE started_at_utc >= ? AND started_at_utc < ?
            ORDER BY started_at_utc, landing_record_id
            "#,
            lower,
            upper
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        let mut read = Vec::new();
        for row in rows {
            let instant: jiff::Timestamp =
                row.started_at.parse().map_err(|error| corrupt(&error))?;
            let zone = OperatorZone::try_from(row.zone).map_err(|error| corrupt(&error))?;
            let started_at = StartedAt::new(instant, zone);
            if !(from..=to).contains(&started_at.wall_clock().date()) {
                continue;
            }
            let duration = u64::try_from(row.duration)
                .map_err(|error| corrupt(&error))
                .and_then(|seconds| {
                    PositiveDuration::from_seconds(seconds).map_err(|error| corrupt(&error))
                })?;
            let heart_rate = match (row.average_bpm, row.highest_bpm) {
                (Some(average), Some(highest)) => {
                    Some(HeartRateSummary::new(beats(average)?, beats(highest)?))
                }
                _ => None,
            };
            read.push(PlayedPadelMatch {
                started_at,
                duration,
                heart_rate,
            });
        }
        Ok(read)
    }
}

/// Each match as History lists it (#817), which is all of it but its highest
/// heart rate.
impl Activities for SqlitePadelMatches {
    async fn between(&self, from: Date, to: Date) -> Result<Vec<(Occurred, Activity)>, StoreError> {
        Ok(PlayedPadelMatches::between(self, from, to)
            .await?
            .into_iter()
            .map(|played| {
                (
                    Occurred::At(played.started_at),
                    Activity::Padel {
                        length: played.duration,
                        heart_rate: played.heart_rate.map(HeartRateSummary::average),
                    },
                )
            })
            .collect())
    }

    async fn first(&self) -> Result<Option<Date>, StoreError> {
        let row = sqlx::query!(
            r#"
            SELECT started_at_utc AS "started_at!: String", zone AS "zone!: String"
              FROM padel_match
             ORDER BY started_at_utc
             LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        row.map(|row| {
            let instant: jiff::Timestamp =
                row.started_at.parse().map_err(|error| corrupt(&error))?;
            let zone = OperatorZone::try_from(row.zone).map_err(|error| corrupt(&error))?;
            Ok(StartedAt::new(instant, zone).wall_clock().date())
        })
        .transpose()
    }
}

fn beats(bpm: i64) -> Result<BeatsPerMinute, StoreError> {
    u32::try_from(bpm)
        .map_err(|error| corrupt(&error))
        .and_then(|bpm| BeatsPerMinute::new(bpm).map_err(|error| corrupt(&error)))
}

/// `padel_match` (#809).
impl application::Input for SqlitePadelMatches {
    fn reads() -> application::Reads {
        super::tables(&[super::table::PADEL_MATCH])
    }
}
