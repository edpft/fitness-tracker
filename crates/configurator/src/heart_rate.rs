//! Each main ride's heart rate, over this build's store (#152, #621).

use application::{StoreError, heart_rate::RideHeartRates, next::Scope};
use domain::{
    analytical::{heart_rate::RideHeartRate, range::Ranges},
    normalised::OperatorZone,
};
use infrastructure::{SqliteCanonicalCyclingSessionStore, SqliteFtpHistory};

use crate::store::Spanned;
use crate::{
    devices,
    graph::{page, read},
    next::NextFailure,
    ranges::{self, Current},
    store::Store,
};

page! {
    /// What the heart rate page reads: the canonical rides, the FTP derived
    /// with them, the monitor each was ridden with and which watch was in use
    /// (#559, #569), and the ranges.
    pub struct Page {
        rides: RideHeartRates<
            Spanned<SqliteCanonicalCyclingSessionStore>,
            Spanned<SqliteFtpHistory>,
            Spanned<infrastructure::GarminRecordedDevices>,
            Spanned<infrastructure::GarminWear>,
        >,
        ranges: Current,
    }
}

/// The parts a run collecting what the page reads takes.
#[must_use]
pub fn collects() -> Scope {
    crate::graph::collects::<Page>()
}

impl Page {
    /// The page over `store`.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn open(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        Ok(Self {
            rides: RideHeartRates::new(
                store.adapter(SqliteCanonicalCyclingSessionStore::new),
                store.adapter(SqliteFtpHistory::new),
                devices::register(store),
            )
            .into(),
            ranges: ranges::over(store, zone).await?.into(),
        })
    }

    /// Every main ride's heart rate, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn rides(&self) -> Result<Vec<RideHeartRate>, StoreError> {
        read!(self.rides.rides()).await
    }

    /// The ranges today.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn ranges(&self, zone: &OperatorZone) -> Result<Ranges, StoreError> {
        read!(self.ranges, |of| ranges::today(of, zone)).await
    }
}
