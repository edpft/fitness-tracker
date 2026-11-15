//! The FTP tests, in watts and over body weight (#469).
//!
//! **Each test's reading is `domain`'s** ([`tests`]); this finds the tests,
//! the FTP each one gave, and hands them over with the canonical weigh-ins.
//! Nothing is written: every point re-derives from them exactly (§ 5).

use domain::analytical::{
    Weighed,
    ftp_test::{Test, TestHeartRate, heart_rate, tests},
};

use crate::{
    devices::DeviceRegister,
    error::StoreError,
    ports::{FtpHistory, PerformedCyclingSessions, RecordedDevices, WearRecord},
};

/// The use case.
pub struct FtpTests<S, F> {
    sessions: S,
    ftp: F,
}

crate::graph::through!(FtpTests<S, F>);

impl<S, F> FtpTests<S, F>
where
    S: PerformedCyclingSessions + Sync,
    F: FtpHistory + Sync,
{
    pub const fn new(sessions: S, ftp: F) -> Self {
        Self { sessions, ftp }
    }

    /// Every test, oldest first, each over the canonical weigh-in `weigh_ins`
    /// places it at.
    ///
    /// **The FTP dated to the test's own day**: one in force from an earlier
    /// day was not this test's, and a test whose effort gave none has no
    /// figure.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn tests(&self, weigh_ins: &[Weighed]) -> Result<Vec<Test>, StoreError> {
        let mut tested = Vec::new();
        for started in self.sessions.tested().await? {
            let day = started.wall_clock().date();
            if let Some(ftp) = self.ftp.in_force_on(day).await?
                && ftp.from() == day
            {
                tested.push((ftp, started));
            }
        }
        Ok(tests(&tested, weigh_ins))
    }
}

/// The heart rate each test was ridden at (#548).
pub struct TestHeartRates<S, R, W> {
    sessions: S,
    devices: DeviceRegister<R, W>,
}

crate::graph::through!(TestHeartRates<S, R, W>);

impl<S, R, W> TestHeartRates<S, R, W>
where
    S: PerformedCyclingSessions + Sync,
    R: RecordedDevices + Sync,
    W: WearRecord + Sync,
{
    pub const fn new(sessions: S, devices: DeviceRegister<R, W>) -> Self {
        Self { sessions, devices }
    }

    /// Every test with heart rate, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn heart_rates(&self) -> Result<Vec<TestHeartRate>, StoreError> {
        let register = self.devices.register().await?;
        let mut rates: Vec<TestHeartRate> = self
            .sessions
            .efforts()
            .await?
            .iter()
            .filter_map(|effort| heart_rate(effort, &register))
            .collect();
        rates.sort_by_key(|rate| rate.on);
        Ok(rates)
    }
}
