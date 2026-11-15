//! Each main ride's heart rate, over this build's store (#152, #621).

use application::{FtpHistory as _, StoreError, heart_rate::RideHeartRates, next::Scope};
use domain::{
    analytical::{heart_rate::RideHeartRate, range::Ranges},
    normalised::OperatorZone,
};
use infrastructure::{SqliteCanonicalCyclingSessionStore, SqliteFtpHistory};

use crate::clock::Today;
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
        rides: RideHeartRates<Spanned<SqliteCanonicalCyclingSessionStore>>,
        ftp_history: Spanned<SqliteFtpHistory>,
        devices: devices::Register,
        ranges: Current,
        today: Today,
    }
}

/// The parts a run collecting what the page reads takes.
#[must_use]
pub fn collects() -> Scope {
    crate::graph::scope(&reads())
}

/// What the page reads, and so what its kept drawing is drawn from (#805).
#[must_use]
pub fn reads() -> application::graph::Reads {
    <Page as application::graph::Input>::reads()
}

impl Page {
    /// The page over `store`.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn open(store: &Store, zone: &OperatorZone) -> Result<Self, NextFailure> {
        Ok(Self {
            rides: RideHeartRates::new(store.adapter(SqliteCanonicalCyclingSessionStore::new))
                .into(),
            ftp_history: store.adapter(SqliteFtpHistory::new).into(),
            devices: devices::register(store).into(),
            ranges: ranges::over(store, zone).await?.into(),
            today: Today::new(zone.clone()).into(),
        })
    }

    /// Every main ride's heart rate, oldest first.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn rides(&self) -> Result<Vec<RideHeartRate>, StoreError> {
        let register = read!(self.devices.register()).await?;
        let ftp = read!(self.ftp_history.series()).await?;
        read!(self.rides.rides(&register, &ftp)).await
    }

    /// The ranges today.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn ranges(&self) -> Result<Ranges, StoreError> {
        let today = self.today.date();
        read!(self.ranges, |of| of.on(today)).await
    }
}
