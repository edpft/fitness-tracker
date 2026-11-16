//! Each main ride's heart rate (#152, #621).
//!
//! **The arithmetic is `domain`'s** ([`ride_heart_rate`]); this finds the
//! rides and the FTP in force on each one's date in the series, and hands
//! them over with the register that says which device sent each one's heart
//! rate. Nothing is written: every point re-derives from them exactly (§ 5).

use domain::{
    analytical::heart_rate::{RideHeartRate, ride_heart_rate},
    cycling::Ftp,
    device::Register,
};

use crate::{error::StoreError, ports::PerformedCyclingSessions};

/// Each main ride's heart rate.
pub struct RideHeartRates<S> {
    sessions: S,
}

crate::graph::through!(RideHeartRates<S>);

impl<S> RideHeartRates<S>
where
    S: PerformedCyclingSessions + Sync,
{
    pub const fn new(sessions: S) -> Self {
        Self { sessions }
    }

    /// Every main ride, oldest first, its heart rate placed on a device by
    /// `register`, against the FTP in force on its day in `ftp`, the series
    /// oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn rides(
        &self,
        register: &Register,
        ftp: &[Ftp],
    ) -> Result<Vec<RideHeartRate>, StoreError> {
        let mut rides = Vec::new();
        for main in self.sessions.main_rides().await? {
            let date = main.started_at().value().wall_clock().date();
            let in_force = Ftp::in_force_on(ftp, date);
            rides.push(ride_heart_rate(&main, in_force, register));
        }
        Ok(rides)
    }
}
