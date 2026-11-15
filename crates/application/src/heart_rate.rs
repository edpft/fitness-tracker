//! Each main ride's heart rate (#152, #621).
//!
//! **The arithmetic is `domain`'s** ([`ride_heart_rate`]); this finds the
//! rides and the FTP in force on each one's date, and hands them over with
//! the register that says which device sent each one's heart rate. Nothing
//! is written: every point re-derives from them exactly (§ 5).

use domain::{
    analytical::heart_rate::{RideHeartRate, ride_heart_rate},
    device::Register,
};

use crate::{
    error::StoreError,
    ports::{FtpHistory, PerformedCyclingSessions},
};

/// Each main ride's heart rate.
pub struct RideHeartRates<S, F> {
    sessions: S,
    ftp: F,
}

crate::graph::through!(RideHeartRates<S, F>);

impl<S, F> RideHeartRates<S, F>
where
    S: PerformedCyclingSessions + Sync,
    F: FtpHistory + Sync,
{
    pub const fn new(sessions: S, ftp: F) -> Self {
        Self { sessions, ftp }
    }

    /// Every main ride, oldest first, its heart rate placed on a device by
    /// `register`.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn rides(&self, register: &Register) -> Result<Vec<RideHeartRate>, StoreError> {
        let mut rides = Vec::new();
        for main in self.sessions.main_rides().await? {
            let date = main.started_at().value().wall_clock().date();
            let ftp = self.ftp.in_force_on(date).await?;
            rides.push(ride_heart_rate(&main, ftp, register));
        }
        Ok(rides)
    }
}
