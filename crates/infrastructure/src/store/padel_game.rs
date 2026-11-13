//! Every padel game, read back for History (#772).

use application::{PlayedPadelGame, PlayedPadelGames, StoreError};
use domain::{
    measure::{BeatsPerMinute, HeartRateSummary, PositiveDuration},
    normalised::{OperatorZone, StartedAt},
};
use sqlx::SqlitePool;

use super::{corrupt, store_error};

/// The read side of the padel games.
#[derive(Debug, Clone)]
pub struct SqlitePadelGames {
    pool: SqlitePool,
}

impl SqlitePadelGames {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl PlayedPadelGames for SqlitePadelGames {
    async fn all(&self) -> Result<Vec<PlayedPadelGame>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT started_at_utc AS "started_at!: String",
                   zone AS "zone!: String",
                   duration_seconds AS "duration!: i64",
                   average_bpm, highest_bpm
            FROM padel_game
            ORDER BY started_at_utc, landing_record_id
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;
        rows.into_iter()
            .map(|row| {
                let instant: jiff::Timestamp =
                    row.started_at.parse().map_err(|error| corrupt(&error))?;
                let zone = OperatorZone::try_from(row.zone).map_err(|error| corrupt(&error))?;
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
                Ok(PlayedPadelGame {
                    started_at: StartedAt::new(instant, zone),
                    duration,
                    heart_rate,
                })
            })
            .collect()
    }
}

fn beats(bpm: i64) -> Result<BeatsPerMinute, StoreError> {
    u32::try_from(bpm)
        .map_err(|error| corrupt(&error))
        .and_then(|bpm| BeatsPerMinute::new(bpm).map_err(|error| corrupt(&error)))
}

/// `padel_game`, which Garmin's activities write (#809).
impl application::Input for SqlitePadelGames {
    fn reads() -> application::Reads {
        super::normalised(&[super::GarminActivityLandingStore::STREAM])
    }
}
