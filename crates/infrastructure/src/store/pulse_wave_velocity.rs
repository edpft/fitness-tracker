//! Pulse wave velocity, read back for the analytical layer (#471).
//!
//! **Every reading the scale took, and the algorithm that took it**: what a
//! series is and how a line runs through it is `domain`'s
//! ([`domain::analytical::pulse_wave_velocity`]). Vascular age stays in the
//! store and is not read.

use application::{PulseWaveVelocityHistory, StoreError};
use domain::{analytical::pulse_wave_velocity::Measured, body::PulseWaveVelocity};
use sqlx::SqlitePool;

use super::{composition::day, corrupt, store_error};

/// Every pulse wave velocity in the store.
#[derive(Debug, Clone)]
pub struct SqlitePulseWaveVelocityHistory {
    pool: SqlitePool,
}

impl SqlitePulseWaveVelocityHistory {
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl PulseWaveVelocityHistory for SqlitePulseWaveVelocityHistory {
    async fn measured(&self) -> Result<Vec<Measured>, StoreError> {
        let rows = sqlx::query!(
            r#"
            SELECT w.on_day AS "on_day: String",
                   w.measured_at_utc AS "measured_at_utc: String",
                   w.zone AS "zone: String",
                   v.pulse_wave_velocity_millimetres_per_second AS "millimetres!: i64",
                   p.algorithm AS "algorithm!: i64"
            FROM body_scan_vascular v
            JOIN body_scan_weigh_in b ON b.landing_record_id = v.weigh_in
            JOIN weigh_in w ON w.id = b.weigh_in
            JOIN body_scan_part p
                ON p.weigh_in = v.weigh_in AND p.part = 'vascular'
            ORDER BY COALESCE(w.measured_at_utc, w.on_day), w.id
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| store_error(&error))?;

        rows.into_iter()
            .map(|row| {
                Ok(Measured {
                    on: day(
                        row.on_day.as_deref(),
                        row.measured_at_utc.as_deref(),
                        row.zone.as_deref(),
                    )?,
                    algorithm: u64::try_from(row.algorithm).map_err(|error| corrupt(&error))?,
                    velocity: PulseWaveVelocity::from_millimetres_per_second(
                        u32::try_from(row.millimetres).map_err(|error| corrupt(&error))?,
                    ),
                })
            })
            .collect()
    }
}
