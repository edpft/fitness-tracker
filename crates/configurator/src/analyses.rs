//! Each analysis this build holds, over this build's store (#830): what it
//! reads the store through, what else it reads, and how it is worked out.
//!
//! **Each is a node in the graph** ([`crate::graph`] declares every one), so a
//! card reading one collects, and is drawn again with, everything it is
//! derived from.

use std::sync::Arc;

use application::{
    FtpHistory as _,
    ftp_tests::TestHeartRates as HeartRatesOfTests,
    graph::Analysis,
    heart_rate::RideHeartRates as HeartRatesOfRides,
    one_rep_max::{Tests, View},
    per_device::PerDevice,
    pulse_wave_velocity::PulseWaveVelocities,
    sleep::Sleeping,
    working::{Frame, IsItWorking, Responding},
};
use domain::{
    analytical::{
        body_weight, composition::ComponentSeries, ftp_test, heart_rate::RideHeartRate,
        heart_rate_at_power, hrv, pulse_wave_velocity, resting, sleep,
    },
    cycling::Ftp,
    device,
    normalised::OperatorZone,
};
use infrastructure::{
    GarminRecordedDevices, GarminWear, SqliteCanonicalCyclingSessionStore, SqliteFtpHistory,
    SqliteOvernightHrvHistory, SqlitePulseWaveVelocityHistory, SqliteRestingHeartRateHistory,
    SqliteSleepHistory, SqliteStressHistory, SqliteTrainingDone,
};
use jiff::civil::Date;

use crate::{
    body_weight as weigh_ins,
    clock::Today,
    composition as components, devices, ftp_tests,
    held::{Analysed, Of},
    next::NextFailure,
    one_rep_max, rescheduling,
    store::{Spanned, Store},
};

/// Every canonical weigh-in, and the trend through them.
pub struct BodyWeight;

impl Analysed for BodyWeight {
    const ANALYSIS: Analysis = Analysis::BodyWeight;
    type Ports = weigh_ins::Series;
    type Reads = ();
    type Value = body_weight::Series;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(weigh_ins::series(store))
    }

    async fn derive(ports: Self::Ports, (): ()) -> Result<Self::Value, NextFailure> {
        Ok(ports.series().await?)
    }
}

/// Every component anything measured, one series per method.
pub struct Composition;

impl Analysed for Composition {
    const ANALYSIS: Analysis = Analysis::Composition;
    type Ports = components::ByMethod;
    type Reads = ();
    type Value = Vec<ComponentSeries>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(components::over(store))
    }

    async fn derive(ports: Self::Ports, (): ()) -> Result<Self::Value, NextFailure> {
        Ok(ports.by_method().await?)
    }
}

/// The plans, as written and as they now stand, and the holidays their term
/// was read in.
pub struct Plans;

impl Analysed for Plans {
    const ANALYSIS: Analysis = Analysis::Plans;
    type Ports = rescheduling::Plans;
    type Reads = ();
    type Value = rescheduling::Plans;

    /// **The walk is the read**: opening the plans walks them.
    async fn open(store: &Store, zone: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        rescheduling::plans(store, zone).await
    }

    async fn derive(ports: Self::Ports, (): ()) -> Result<Self::Value, NextFailure> {
        Ok(ports)
    }
}

/// Every device the record identifies, and the days a watch was worn.
pub struct Register;

impl Analysed for Register {
    const ANALYSIS: Analysis = Analysis::Register;
    type Ports = devices::Register;
    type Reads = ();
    type Value = device::Register;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(devices::register(store))
    }

    async fn derive(ports: Self::Ports, (): ()) -> Result<Self::Value, NextFailure> {
        Ok(ports.register().await?)
    }
}

/// The FTP, as it has been tested, oldest first.
pub struct FtpHistory;

impl Analysed for FtpHistory {
    const ANALYSIS: Analysis = Analysis::FtpHistory;
    type Ports = Spanned<SqliteFtpHistory>;
    type Reads = ();
    type Value = Vec<Ftp>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(store.adapter(SqliteFtpHistory::new))
    }

    async fn derive(ports: Self::Ports, (): ()) -> Result<Self::Value, NextFailure> {
        Ok(ports.series().await?)
    }
}

/// Every FTP test, oldest first, in watts and over body weight.
pub struct FtpTests;

impl Analysed for FtpTests {
    const ANALYSIS: Analysis = Analysis::FtpTests;
    type Ports = ftp_tests::Tests;
    type Reads = (Of<FtpHistory>, Of<BodyWeight>);
    type Value = Vec<ftp_test::Test>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(ftp_tests::over(store))
    }

    async fn derive(
        ports: Self::Ports,
        (ftp, weight): (Arc<Vec<Ftp>>, Arc<body_weight::Series>),
    ) -> Result<Self::Value, NextFailure> {
        Ok(ports.tests(&ftp, &weight.weigh_ins).await?)
    }
}

/// The heart rate each FTP test was ridden at, oldest first (#548).
pub struct TestHeartRates;

impl Analysed for TestHeartRates {
    const ANALYSIS: Analysis = Analysis::TestHeartRates;
    type Ports = HeartRatesOfTests<
        Spanned<SqliteCanonicalCyclingSessionStore>,
        Spanned<GarminRecordedDevices>,
        Spanned<GarminWear>,
    >;
    type Reads = ();
    type Value = Vec<ftp_test::TestHeartRate>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(HeartRatesOfTests::new(
            store.adapter(SqliteCanonicalCyclingSessionStore::new),
            devices::register(store),
        ))
    }

    async fn derive(ports: Self::Ports, (): ()) -> Result<Self::Value, NextFailure> {
        Ok(ports.heart_rates().await?)
    }
}

/// Every main ride's heart rate, oldest first, on the device that sent it and
/// against the FTP in force on its day.
pub struct RideHeartRates;

impl Analysed for RideHeartRates {
    const ANALYSIS: Analysis = Analysis::RideHeartRates;
    type Ports = HeartRatesOfRides<Spanned<SqliteCanonicalCyclingSessionStore>>;
    type Reads = (Of<Register>, Of<FtpHistory>);
    type Value = Vec<RideHeartRate>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(HeartRatesOfRides::new(
            store.adapter(SqliteCanonicalCyclingSessionStore::new),
        ))
    }

    async fn derive(
        ports: Self::Ports,
        (register, ftp): (Arc<device::Register>, Arc<Vec<Ftp>>),
    ) -> Result<Self::Value, NextFailure> {
        Ok(ports.rides(&register, &ftp).await?)
    }
}

/// Heart rate at a fixed power, each device's rides and the line through them.
pub struct HeartRateAtPower;

impl Analysed for HeartRateAtPower {
    const ANALYSIS: Analysis = Analysis::HeartRateAtPower;
    type Ports = ();
    type Reads = Of<RideHeartRates>;
    type Value = Vec<heart_rate_at_power::Trended>;

    async fn open(_: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(())
    }

    async fn derive(
        (): Self::Ports,
        rides: Arc<Vec<RideHeartRate>>,
    ) -> Result<Self::Value, NextFailure> {
        Ok(heart_rate_at_power::trended(&rides))
    }
}

/// Each lift's 1RM tests through today, and each lift's estimates over the
/// macrocycle in force, from one read of the lifts' performances (#817).
pub struct Strength;

impl Analysed for Strength {
    const ANALYSIS: Analysis = Analysis::Strength;
    type Ports = one_rep_max::Strength;
    type Reads = (Of<Plans>, Of<BodyWeight>, Today);
    type Value = (Vec<Tests>, View);

    async fn open(store: &Store, zone: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(application::one_rep_max::Strength::new(
            one_rep_max::estimated(store),
            one_rep_max::tests(store, zone),
        ))
    }

    async fn derive(
        ports: Self::Ports,
        (plans, weight, today): (Arc<rescheduling::Plans>, Arc<body_weight::Series>, Date),
    ) -> Result<Self::Value, NextFailure> {
        Ok(ports
            .read(today, plans.written(), plans.now(), &weight.weigh_ins)
            .await?)
    }
}

/// Resting heart rate, overnight HRV and stress over the macrocycle in force,
/// each against the training done: `None` on a day no plan is in force.
pub struct Responses;

impl Analysed for Responses {
    const ANALYSIS: Analysis = Analysis::Responses;
    type Ports = IsItWorking<
        Spanned<SqliteOvernightHrvHistory>,
        Spanned<SqliteRestingHeartRateHistory>,
        Spanned<SqliteStressHistory>,
        Spanned<SqliteTrainingDone>,
    >;
    type Reads = (Of<Plans>, Of<Register>, Today);
    type Value = Option<Vec<Responding>>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(IsItWorking::new(
            store.adapter(SqliteOvernightHrvHistory::new),
            store.adapter(SqliteRestingHeartRateHistory::new),
            store.adapter(SqliteStressHistory::new),
            store.adapter(SqliteTrainingDone::new),
        ))
    }

    async fn derive(
        ports: Self::Ports,
        (plans, register, today): (Arc<rescheduling::Plans>, Arc<device::Register>, Date),
    ) -> Result<Self::Value, NextFailure> {
        let Some(frame) = Frame::on(today, plans.now()) else {
            return Ok(None);
        };
        Ok(Some(ports.responses(frame.began, today, &register).await?))
    }
}

/// Last night's average HRV, one series per watch, and the trend through each.
pub struct OvernightHrv;

impl Analysed for OvernightHrv {
    const ANALYSIS: Analysis = Analysis::OvernightHrv;
    type Ports = PerDevice<
        Spanned<SqliteOvernightHrvHistory>,
        (),
        Spanned<GarminRecordedDevices>,
        Spanned<GarminWear>,
    >;
    type Reads = ();
    type Value = Vec<hrv::Trended>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(PerDevice::new(
            store.adapter(SqliteOvernightHrvHistory::new),
            (),
            devices::register(store),
        ))
    }

    async fn derive(ports: Self::Ports, (): ()) -> Result<Self::Value, NextFailure> {
        Ok(ports.overnight_hrv_trend().await?)
    }
}

/// Each day's resting heart rate, one series per watch, with its slow line and
/// the nights it flags.
pub struct RestingHeartRate;

impl Analysed for RestingHeartRate {
    const ANALYSIS: Analysis = Analysis::RestingHeartRate;
    type Ports = PerDevice<
        (),
        Spanned<SqliteRestingHeartRateHistory>,
        Spanned<GarminRecordedDevices>,
        Spanned<GarminWear>,
    >;
    type Reads = ();
    type Value = Vec<resting::Trended>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(PerDevice::new(
            (),
            store.adapter(SqliteRestingHeartRateHistory::new),
            devices::register(store),
        ))
    }

    async fn derive(ports: Self::Ports, (): ()) -> Result<Self::Value, NextFailure> {
        Ok(ports.resting_heart_rate_trend().await?)
    }
}

/// Every night's sleep, one series per watch, with its trends.
pub struct Sleep;

impl Analysed for Sleep {
    const ANALYSIS: Analysis = Analysis::Sleep;
    type Ports =
        Sleeping<Spanned<SqliteSleepHistory>, Spanned<GarminRecordedDevices>, Spanned<GarminWear>>;
    type Reads = ();
    type Value = Vec<sleep::Trended>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(Sleeping::new(
            store.adapter(SqliteSleepHistory::new),
            devices::register(store),
        ))
    }

    async fn derive(ports: Self::Ports, (): ()) -> Result<Self::Value, NextFailure> {
        Ok(ports.nights().await?)
    }
}

/// Every pulse wave velocity reading, one series per algorithm, with the line
/// through it.
pub struct PulseWaveVelocity;

impl Analysed for PulseWaveVelocity {
    const ANALYSIS: Analysis = Analysis::PulseWaveVelocity;
    type Ports = PulseWaveVelocities<Spanned<SqlitePulseWaveVelocityHistory>>;
    type Reads = ();
    type Value = Vec<pulse_wave_velocity::Series>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(PulseWaveVelocities::new(
            store.adapter(SqlitePulseWaveVelocityHistory::new),
        ))
    }

    async fn derive(ports: Self::Ports, (): ()) -> Result<Self::Value, NextFailure> {
        Ok(ports.by_algorithm().await?)
    }
}

/// Every analysis's derivation, for the graph.
pub(crate) fn derivations() -> [application::graph::Derivation; Analysis::ALL.len()] {
    use crate::held::derivation;
    [
        derivation::<BodyWeight>(),
        derivation::<Composition>(),
        derivation::<Plans>(),
        derivation::<Register>(),
        derivation::<FtpHistory>(),
        derivation::<FtpTests>(),
        derivation::<TestHeartRates>(),
        derivation::<RideHeartRates>(),
        derivation::<HeartRateAtPower>(),
        derivation::<Strength>(),
        derivation::<Responses>(),
        derivation::<OvernightHrv>(),
        derivation::<RestingHeartRate>(),
        derivation::<Sleep>(),
        derivation::<PulseWaveVelocity>(),
    ]
}
