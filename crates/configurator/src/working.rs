//! Is what I'm doing working?, for the macrocycle in force, over this build's
//! store (#517): what Progress shows (#769).

use application::{
    FtpHistory as _, StoreError,
    heart_rate::RideHeartRates,
    next::Scope,
    one_rep_max::Strength,
    working::{IsItWorking, Readings, View},
};
use domain::{analytical::body_weight::Series, normalised::OperatorZone};
use infrastructure::{
    SqliteCanonicalCyclingSessionStore, SqliteFtpHistory, SqliteOvernightHrvHistory,
    SqliteRestingHeartRateHistory, SqliteStressHistory, SqliteTrainingDone,
};

use crate::clock::Today;
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
    /// the weigh-ins, the register, the FTP series and the plans, each read
    /// once for every reading over them (#817).
    pub struct Page {
        working: IsItWorking<
            Spanned<SqliteOvernightHrvHistory>,
            Spanned<SqliteRestingHeartRateHistory>,
            Spanned<SqliteStressHistory>,
            Spanned<SqliteTrainingDone>,
        >,
        ftp: ftp_tests::Tests,
        strength: one_rep_max::Strength,
        rides: RideHeartRates<Spanned<SqliteCanonicalCyclingSessionStore>>,
        ftp_history: Spanned<SqliteFtpHistory>,
        weight: body_weight::Series,
        devices: devices::Register,
        plans: Plans,
        aims: Aimed,
        composition: ByMethod,
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
            working: IsItWorking::new(
                store.adapter(SqliteOvernightHrvHistory::new),
                store.adapter(SqliteRestingHeartRateHistory::new),
                store.adapter(SqliteStressHistory::new),
                store.adapter(SqliteTrainingDone::new),
            )
            .into(),
            ftp: ftp_tests::over(store).into(),
            strength: Strength::new(
                one_rep_max::estimated(store),
                one_rep_max::tests(store, zone),
            )
            .into(),
            rides: RideHeartRates::new(store.adapter(SqliteCanonicalCyclingSessionStore::new))
                .into(),
            ftp_history: store.adapter(SqliteFtpHistory::new).into(),
            weight: body_weight::series(store).into(),
            devices: devices::register(store).into(),
            plans: rescheduling::plans(store, zone).await?.into(),
            aims: body_weight::aims_over(store).into(),
            composition: composition::over(store).into(),
            today: Today::new(zone.clone()).into(),
        })
    }

    /// The macrocycle in force, through today, its mesocycles as the plans
    /// now stand (#800).
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store cannot be read.
    pub async fn view(&self) -> Result<View, StoreError> {
        let today = self.today.date();
        let weigh_ins = read!(self.weight.weigh_ins()).await?;
        let register = read!(self.devices.register()).await?;
        let (written, plans) = read!(self.plans.written_and_now()).await?;
        let ftp_history = read!(self.ftp_history.series()).await?;
        let ftp = read!(self.ftp.tests(&ftp_history, &weigh_ins)).await?;
        let (strength, estimated) =
            read!(self.strength.read(today, &written, &plans, &weigh_ins)).await?;
        let estimated = estimated.lifts;
        let at_power = domain::analytical::heart_rate_at_power::trended(
            &read!(self.rides.rides(&register, &ftp_history)).await?,
        );
        let weight = Series::of(weigh_ins);
        let aims = read!(self.aims.on(today, &plans)).await?;
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
        read!(self.working.view(today, &plans, &register, &readings)).await
    }
}
