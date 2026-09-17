//! Relative strength: estimated one-rep maximum over body weight (#153).
//!
//! **The arithmetic is `domain`'s** ([`relative_strength`]); this finds the two
//! records it reads and hands them over. Nothing is written: the figure
//! re-derives from them exactly (§ 5).

use domain::analytical::{OneRepMaxEstimator, SessionStrength, relative_strength};
use jiff::civil::{Date, date};

use crate::{
    error::StoreError,
    ports::{PerformedWorkoutReader, WeighInHistory},
};

/// Before anything the record holds. The session reader takes a window, and
/// this report wants all of it.
const EARLIEST: Date = date(2000, 1, 1);

/// What the report says, and which series it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrengthReport {
    /// The estimator's name. A different estimator is a different series (§ 6).
    pub estimator: &'static str,
    pub rows: Vec<SessionStrength>,
}

/// The report use case.
pub struct RelativeStrength<E, S, W> {
    estimator: E,
    sessions: S,
    weigh_ins: W,
}

impl<E, S, W> RelativeStrength<E, S, W>
where
    E: OneRepMaxEstimator + Sync,
    S: PerformedWorkoutReader + Sync,
    W: WeighInHistory + Sync,
{
    pub const fn new(estimator: E, sessions: S, weigh_ins: W) -> Self {
        Self {
            estimator,
            sessions,
            weigh_ins,
        }
    }

    /// Every session through `through` in which a headline lift was performed.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if either store is unavailable.
    pub async fn report(&self, through: Date) -> Result<StrengthReport, StoreError> {
        let sessions = self.sessions.between(EARLIEST, through).await?;
        let weigh_ins = self.weigh_ins.weigh_ins().await?;
        Ok(StrengthReport {
            estimator: E::NAME,
            rows: relative_strength(&self.estimator, &sessions, &weigh_ins),
        })
    }
}
