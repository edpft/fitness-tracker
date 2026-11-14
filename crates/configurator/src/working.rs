//! Is what I'm doing working?, for the macrocycle in force, over this build's
//! store (#517): what Progress shows (#769).

use application::{
    StoreError,
    heart_rate::RideHeartRates,
    next::Scope,
    working::{IsItWorking, Readings, View},
};
use domain::normalised::OperatorZone;
use infrastructure::{
    GarminRecordedDevices, GarminWear, SqliteCanonicalCyclingSessionStore, SqliteFtpHistory,
    SqliteOvernightHrvHistory, SqliteRestingHeartRateHistory, SqliteStressHistory,
    SqliteTrainingDone,
};

use crate::store::Spanned;
use crate::{
    body_weight::{self, Aimed},
    composition::{self, ByMethod},
    devices, ftp_tests,
    graph::{page, read},
    next::NextFailure,
    one_rep_max, rescheduling,
    rescheduling::Plans,
    store::Store,
};

page! {
    /// What Progress reads: the three response readings, each normalised; the
    /// training done; the FTP tests, the 1RM tests and estimates, heart rate
    /// at a fixed power, body weight and its aims, and body composition; and
    /// the register.
    pub struct Page {
        working: IsItWorking<
            Spanned<SqliteOvernightHrvHistory>,
            Spanned<SqliteRestingHeartRateHistory>,
            Spanned<SqliteStressHistory>,
            Spanned<SqliteTrainingDone>,
            Spanned<GarminRecordedDevices>,
            Spanned<GarminWear>,
            Plans,
        >,
        ftp: ftp_tests::Tests,
        strength: one_rep_max::Tests,
        estimated: one_rep_max::Estimated,
        rides: RideHeartRates<
            Spanned<SqliteCanonicalCyclingSessionStore>,
            Spanned<SqliteFtpHistory>,
            Spanned<GarminRecordedDevices>,
            Spanned<GarminWear>,
        >,
        weight: body_weight::Series,
        aims: Aimed,
        composition: ByMethod,
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
        let plans = rescheduling::plans(store, zone).await?;
        Ok(Self {
            working: IsItWorking::new(
                store.adapter(SqliteOvernightHrvHistory::new),
                store.adapter(SqliteRestingHeartRateHistory::new),
                store.adapter(SqliteStressHistory::new),
                store.adapter(SqliteTrainingDone::new),
                devices::register(store),
                plans.clone(),
            )
            .into(),
            ftp: ftp_tests::over(store).into(),
            strength: one_rep_max::tests(store, zone).into(),
            estimated: one_rep_max::estimated(store, plans.clone()).into(),
            rides: RideHeartRates::new(
                store.adapter(SqliteCanonicalCyclingSessionStore::new),
                store.adapter(SqliteFtpHistory::new),
                devices::register(store),
            )
            .into(),
            weight: body_weight::series(store).into(),
            aims: body_weight::aims_over(store, plans).into(),
            composition: composition::over(store).into(),
        })
    }

    /// The macrocycle in force, through today, its mesocycles as the plans
    /// now stand (#800).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn view(&self, zone: &OperatorZone) -> Result<View, StoreError> {
        let today = crate::equipment::today(zone);
        let ftp = read!(self.ftp.tests()).await?;
        let strength = read!(self.strength.tests(today)).await?;
        let estimated = read!(self.estimated.view(today)).await?.lifts;
        let at_power =
            domain::analytical::heart_rate_at_power::trended(&read!(self.rides.rides()).await?);
        let weight = read!(self.weight.series()).await?;
        let aims = read!(self.aims.on(today)).await?;
        let composition = read!(self.composition.by_method()).await?;
        let readings = Readings {
            ftp: &ftp,
            strength: &strength,
            estimated: &estimated,
            at_power: &at_power,
            weight: &weight,
            aims: &aims,
            composition: &composition,
        };
        read!(self.working.view(today, &readings)).await
    }
}
