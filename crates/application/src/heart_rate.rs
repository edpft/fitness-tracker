//! Heart rate per power zone, for each main ride (#152).
//!
//! **The arithmetic is `domain`'s** ([`heart_rate_by_zone`]); this finds the
//! rides, the FTP in force on each one's date, what each one's class
//! prescribed, and the device that sent each one's heart rate, and hands them
//! over. Nothing is written: every point
//! re-derives from them exactly (§ 5).

use domain::{
    analytical::heart_rate::{RideHeartRate, heart_rate_by_zone},
    cycling::CanonicalCyclingSession,
};

use crate::{
    devices::DeviceRegister,
    error::StoreError,
    ports::{
        ClassPrescriptions, FtpHistory, PerformedCyclingSessions, RecordedDevices, WearRecord,
    },
};

/// The heart-rate-per-zone use case.
pub struct HeartRatePerZone<S, F, C, R, W> {
    sessions: S,
    ftp: F,
    classes: C,
    devices: DeviceRegister<R, W>,
}

impl<S, F, C, R, W> HeartRatePerZone<S, F, C, R, W>
where
    S: PerformedCyclingSessions + Sync,
    F: FtpHistory + Sync,
    C: ClassPrescriptions + Sync,
    R: RecordedDevices + Sync,
    W: WearRecord + Sync,
{
    pub const fn new(sessions: S, ftp: F, classes: C, devices: DeviceRegister<R, W>) -> Self {
        Self {
            sessions,
            ftp,
            classes,
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
            let prescribed = match &session {
                CanonicalCyclingSession::Ride { main, .. } => {
                    self.classes.prescribed(main.at().value()).await?
                }
                CanonicalCyclingSession::Test { .. } => None,
            };
            rides.extend(heart_rate_by_zone(
                &session,
                ftp,
                prescribed.as_ref(),
                &register,
            ));
        }
        Ok(rides)
    }
}
