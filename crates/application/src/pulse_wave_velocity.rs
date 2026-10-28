//! Pulse wave velocity, as a slow line (#471).
//!
//! **The series are `domain`'s** ([`by_algorithm`]); this reads the readings
//! and hands them over. Nothing is written: every line re-derives from them
//! exactly (§ 5).

use domain::analytical::pulse_wave_velocity::{Series, by_algorithm};

use crate::{error::StoreError, ports::PulseWaveVelocityHistory};

/// The use case.
pub struct PulseWaveVelocities<H> {
    history: H,
}

impl<H> PulseWaveVelocities<H>
where
    H: PulseWaveVelocityHistory + Sync,
{
    pub const fn new(history: H) -> Self {
        Self { history }
    }

    /// Every reading, one series per algorithm, with the line through it.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn by_algorithm(&self) -> Result<Vec<Series>, StoreError> {
        Ok(by_algorithm(&self.history.measured().await?))
    }
}
