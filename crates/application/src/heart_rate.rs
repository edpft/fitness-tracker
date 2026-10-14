//! Heart rate per power zone, for each main ride (#152).
//!
//! **The arithmetic is `domain`'s** ([`heart_rate_by_zone`]); this finds the
//! rides, the FTP in force on each one's date, and the device that sent each
//! one's heart rate, and hands them over. Nothing is written: every point
//! re-derives from them exactly (§ 5).

use domain::analytical::heart_rate::{RideHeartRate, heart_rate_by_zone};

use crate::{
    devices::DeviceRegister,
    error::StoreError,
    ports::{FtpHistory, PerformedCyclingSessions, RecordedDevices, WearRecord},
};

/// The heart-rate-per-zone use case.
pub struct HeartRatePerZone<S, F, R, W> {
    sessions: S,
    ftp: F,
    devices: DeviceRegister<R, W>,
}

impl<S, F, R, W> HeartRatePerZone<S, F, R, W>
where
    S: PerformedCyclingSessions + Sync,
    F: FtpHistory + Sync,
    R: RecordedDevices + Sync,
    W: WearRecord + Sync,
{
    pub const fn new(sessions: S, ftp: F, devices: DeviceRegister<R, W>) -> Self {
        Self {
            sessions,
            ftp,
            devices,
        }
    }

    /// Every main ride, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn rides(&self) -> Result<Vec<RideHeartRate>, StoreError> {
        let register = self.devices.register().await?;
        let mut rides = Vec::new();
        for session in self.sessions.all().await? {
            let date = session.started_at().wall_clock().date();
            let ftp = self.ftp.in_force_on(date).await?;
            rides.extend(heart_rate_by_zone(&session, ftp, &register));
        }
        Ok(rides)
    }
}
