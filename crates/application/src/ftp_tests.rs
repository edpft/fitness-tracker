//! The FTP tests, in watts and over body weight (#469).
//!
//! **Each test's reading is `domain`'s** ([`tests`]); this finds the tests,
//! the FTP each one gave and the canonical weigh-ins, and hands them over.
//! Nothing is written: every point re-derives from them exactly (§ 5).

use domain::{
    analytical::ftp_test::{Test, tests},
    cycling::CanonicalCyclingSession,
};

use crate::{
    error::StoreError,
    ports::{FtpHistory, PerformedCyclingSessions, WeighInHistory},
};

/// The use case.
pub struct FtpTests<S, F, W> {
    sessions: S,
    ftp: F,
    weigh_ins: W,
}

impl<S, F, W> FtpTests<S, F, W>
where
    S: PerformedCyclingSessions + Sync,
    F: FtpHistory + Sync,
    W: WeighInHistory + Sync,
{
    pub const fn new(sessions: S, ftp: F, weigh_ins: W) -> Self {
        Self {
            sessions,
            ftp,
            weigh_ins,
        }
    }

    /// Every test, oldest first.
    ///
    /// **The FTP dated to the test's own day**: one in force from an earlier
    /// day was not this test's, and a test whose effort gave none has no
    /// figure.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if any store is unavailable.
    pub async fn tests(&self) -> Result<Vec<Test>, StoreError> {
        let mut tested = Vec::new();
        for session in self.sessions.all().await? {
            if !matches!(session, CanonicalCyclingSession::Test { .. }) {
                continue;
            }
            let started = session.started_at();
            let day = started.wall_clock().date();
            if let Some(ftp) = self.ftp.in_force_on(day).await?
                && ftp.from() == day
            {
                tested.push((ftp, started.clone()));
            }
        }
        Ok(tests(&tested, &self.weigh_ins.weigh_ins().await?))
    }
}
