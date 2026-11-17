//! Each analysis this build holds, over this build's store (#830): what it
//! reads the store through, what else it reads, and how it is worked out.
//!
//! **Each is a node in the graph** ([`crate::graph`] declares every one), so a
//! card reading one collects, and is drawn again with, everything it is
//! derived from.

use std::sync::Arc;

use application::{
    FtpHistory as _, StressHistory as _, TrainingDone as _,
    ftp_tests::TestHeartRates as HeartRatesOfTests,
    graph::Analysis,
    heart_rate::RideHeartRates as HeartRatesOfRides,
    next::Upcoming,
    one_rep_max::{Tests, View},
    per_device::PerDevice,
    pulse_wave_velocity::PulseWaveVelocities,
    sleep::Sleeping,
    working::{self, Frame, Framed, Performance, Responding, SoFar, Tested},
};
use domain::{
    analytical::{
        body_weight::{self, Aim},
        composition::ComponentSeries,
        ftp_test,
        heart_rate::RideHeartRate,
        heart_rate_at_power, hrv,
        per_device::{DeviceSeries, per_device},
        pulse_wave_velocity, range, resting,
        since_entry::SinceEntry,
        since_start::SinceStart,
        sleep, stress,
        training_done::Carried,
    },
    body::Component,
    cycling::Ftp,
    device::{self, Measurand},
    normalised::OperatorZone,
    schedule::DayPart,
};
use infrastructure::{
    GarminRecordedDevices, GarminWear, SqliteCanonicalCyclingSessionStore, SqliteFtpHistory,
    SqliteOvernightHrvHistory, SqlitePulseWaveVelocityHistory, SqliteRestingHeartRateHistory,
    SqliteSleepHistory, SqliteStressHistory, SqliteTrainingDone,
};
use jiff::civil::Date;

use crate::{
    body_weight as weigh_ins,
    clock::{Now, Today},
    composition as components, devices, ftp_tests,
    held::{Analysed, Consulted, Of},
    macrocycle::Stood,
    next::NextFailure,
    one_rep_max, rescheduling,
    store::{Spanned, Store},
};

/// Each lift's 1RM tests, and its estimates over the macrocycle.
pub type Lifts = (Vec<Tests>, View);

/// A response reading over the macrocycle in force: `None` on a day no plan
/// is in force.
pub type Response = Option<Framed<Responding>>;

/// Every response reading over the macrocycle in force.
pub type ResponsesRead = Option<Framed<Vec<Responding>>>;

/// Every test since its entry test, each estimate since the start, and every
/// response reading: what fitness so far is read from.
type Performed = (
    Arc<Option<SinceEntry>>,
    Arc<Option<SinceStart>>,
    Arc<Vec<Performance>>,
    Arc<ResponsesRead>,
);

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
    /// **Body weight consulted**: each test is read against the weigh-in on
    /// its own day, which a later weigh-in does not change.
    type Reads = (Of<FtpHistory>, Consulted<BodyWeight>);
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
    /// **Body weight consulted**, as the FTP tests consult it.
    type Reads = (Of<Plans>, Consulted<BodyWeight>, Today);
    type Value = Lifts;

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

/// The training each day carries, through today.
pub struct Training;

impl Analysed for Training {
    const ANALYSIS: Analysis = Analysis::Training;
    type Ports = Spanned<SqliteTrainingDone>;
    type Reads = Today;
    type Value = Carried;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(store.adapter(SqliteTrainingDone::new))
    }

    async fn derive(ports: Self::Ports, today: Date) -> Result<Self::Value, NextFailure> {
        Ok(Carried::through(&ports.sessions().await?, today))
    }
}

/// Each day's stress, one series per watch.
pub struct Stress;

impl Analysed for Stress {
    const ANALYSIS: Analysis = Analysis::Stress;
    type Ports = Spanned<SqliteStressHistory>;
    type Reads = Of<Register>;
    type Value = Vec<DeviceSeries<stress::Measured>>;

    async fn open(store: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(store.adapter(SqliteStressHistory::new))
    }

    async fn derive(
        ports: Self::Ports,
        register: Arc<device::Register>,
    ) -> Result<Self::Value, NextFailure> {
        Ok(per_device(
            ports.days().await?,
            Measurand::HeartRate,
            &register,
        ))
    }
}

/// A response reading over the macrocycle in force, against the training
/// done: `None` on a day no plan is in force.
macro_rules! response {
    ($(#[$meta:meta])* $node:ident, $series:ty, $read:path) => {
        $(#[$meta])*
        pub struct $node;

        impl Analysed for $node {
            const ANALYSIS: Analysis = Analysis::$node;
            type Ports = ();
            type Reads = (Of<Macrocycle>, Of<$series>, Of<Training>);
            type Value = Response;

            async fn open(_: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
                Ok(())
            }

            async fn derive(
                (): Self::Ports,
                (frame, series, carried): (
                    Arc<Option<Frame>>,
                    Arc<<$series as Analysed>::Value>,
                    Arc<Carried>,
                ),
            ) -> Result<Self::Value, NextFailure> {
                Ok(frame.as_ref().as_ref().map(|frame| Framed {
                    frame: frame.clone(),
                    read: $read(&series, &carried, frame.began, frame.today),
                }))
            }
        }
    };
}

response!(
    /// Resting heart rate against the training done.
    RestingHeartRateResponse,
    RestingHeartRate,
    working::resting
);
response!(
    /// Overnight HRV against the training done.
    OvernightHrvResponse,
    OvernightHrv,
    working::overnight_hrv
);
response!(
    /// Stress against the training done.
    StressResponse,
    Stress,
    working::stress
);

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

/// An analysis worked out from other analyses alone, reading no port: `$value`
/// is `$derive` of what `$reads` fetches.
macro_rules! composed {
    (
        $(#[$meta:meta])*
        $node:ident: $value:ty = $derive:expr, from $reads:ty $(,)?
    ) => {
        $(#[$meta])*
        pub struct $node;

        impl Analysed for $node {
            const ANALYSIS: Analysis = Analysis::$node;
            type Ports = ();
            type Reads = $reads;
            type Value = $value;

            async fn open(_: &Store, _: &OperatorZone) -> Result<Self::Ports, NextFailure> {
                Ok(())
            }

            async fn derive(
                (): Self::Ports,
                read: <$reads as crate::held::Fetch>::Fetched,
            ) -> Result<Self::Value, NextFailure> {
                Ok($derive(read))
            }
        }
    };
}

composed!(
    /// The macrocycle in force today: `None` on a day no plan is in force.
    Macrocycle: Option<Frame> = |(plans, today): (Arc<rescheduling::Plans>, Date)| {
        Frame::on(today, plans.now())
    },
    from (Of<Plans>, Today)
);

composed!(
    /// The ranges a chart can be read over today, which every chart offering
    /// a range reads.
    Ranges: range::Ranges = |(plans, today): (Arc<rescheduling::Plans>, Date)| {
        application::ranges::of(today, plans.now())
    },
    from (Of<Plans>, Today)
);

composed!(
    /// The plan's aims for body weight, as they stand today.
    Aims: Vec<Aim> = |(plans, today): (Arc<rescheduling::Plans>, Date)| {
        application::body_weight::aims_on(today, plans.now(), plans.holidays())
    },
    from (Of<Plans>, Today)
);

composed!(
    /// The latest FTP test against the macrocycle's entry test: `None` where
    /// no plan is in force or it holds no test.
    FtpSinceEntry: Option<SinceEntry> =
        |(frame, tests): (Arc<Option<Frame>>, Arc<Vec<ftp_test::Test>>)| {
            frame
                .as_ref()
                .as_ref()
                .and_then(|frame| working::ftp(&tests, frame.began, frame.today))
        },
    from (Of<Macrocycle>, Of<FtpTests>)
);

composed!(
    /// Heart rate at 140 W since the macrocycle's start, on the line of the
    /// device that rode last.
    AtPowerSinceStart: Option<SinceStart> =
        |(frame, rides): (Arc<Option<Frame>>, Arc<Vec<heart_rate_at_power::Trended>>)| {
            frame
                .as_ref()
                .as_ref()
                .and_then(|frame| working::at_power(&rides, frame.began, frame.today))
        },
    from (Of<Macrocycle>, Of<HeartRateAtPower>)
);

composed!(
    /// Body weight's trend since the macrocycle's start, against where the
    /// plan's aims would have taken it.
    BodyWeightAgainstAims: Option<Framed<body_weight::Weight>> =
        |(frame, aims, series): (Arc<Option<Frame>>, Arc<Vec<Aim>>, Arc<body_weight::Series>)| {
            let frame = frame.as_ref().as_ref()?;
            body_weight::against_aims(&series.trend, &aims, frame.began, frame.today).map(|read| {
                Framed {
                    frame: frame.clone(),
                    read,
                }
            })
        },
    from (Of<Macrocycle>, Of<Aims>, Of<BodyWeight>)
);

/// A component's mass since the macrocycle's start, by the method whose
/// trend is the latest.
fn since_start(
    (frame, components): (Arc<Option<Frame>>, Arc<Vec<ComponentSeries>>),
    component: Component,
) -> Option<SinceStart> {
    let frame = frame.as_ref().as_ref()?;
    working::component(&components, component, frame.began, frame.today)
}

composed!(
    /// Fat mass since the macrocycle's start.
    FatSinceStart: Option<SinceStart> = |read| since_start(read, Component::Fat),
    from (Of<Macrocycle>, Of<Composition>)
);

composed!(
    /// Fat-free mass since the macrocycle's start.
    FatFreeSinceStart: Option<SinceStart> = |read| since_start(read, Component::FatFree),
    from (Of<Macrocycle>, Of<Composition>)
);

composed!(
    /// Each lift's 1RM test against its entry test, with its estimate since
    /// the start beside it, and each lift estimated but never tested: none
    /// where no plan is in force.
    GymPerformance: Vec<Performance> =
        |(frame, strength): (Arc<Option<Frame>>, Arc<Lifts>)| {
            frame.as_ref().as_ref().map_or_else(Vec::new, |frame| {
                working::gym(&strength.0, &strength.1.lifts, frame.began, frame.today)
            })
        },
    from (Of<Macrocycle>, Of<Strength>)
);

composed!(
    /// Each response reading over the macrocycle there is, in order.
    Responses: ResponsesRead = |(frame, resting, hrv, stress): (
        Arc<Option<Frame>>,
        Arc<Response>,
        Arc<Response>,
        Arc<Response>,
    )| {
        frame.as_ref().as_ref().map(|frame| Framed {
            frame: frame.clone(),
            read: [resting, hrv, stress]
                .iter()
                .filter_map(|one| Some(one.as_ref().as_ref()?.read.clone()))
                .collect(),
        })
    },
    from (
        Of<Macrocycle>,
        Of<RestingHeartRateResponse>,
        Of<OvernightHrvResponse>,
        Of<StressResponse>,
    )
);

composed!(
    /// Has this macrocycle improved my fitness so far? The FTP test with heart
    /// rate at 140 W beside it, each lift, and each response reading.
    FitnessSoFar: Option<Framed<SoFar>> = |(ftp, at_power, gym, responses): Performed| {
        let responses = responses.as_ref().as_ref()?;
        let cycling = Performance {
            tested: Tested::Ftp,
            since: *ftp,
            estimated: *at_power,
        };
        Some(Framed {
            frame: responses.frame.clone(),
            read: SoFar {
                performance: std::iter::once(cycling).chain(gym.iter().copied()).collect(),
                responses: responses.read.clone(),
            },
        })
    },
    from (
        Of<FtpSinceEntry>,
        Of<AtPowerSinceStart>,
        Of<GymPerformance>,
        Of<Responses>,
    )
);

/// The heart rate each FTP test was ridden at, and the days the tests span,
/// which its chart is drawn over so it lines up with the tests'.
pub struct TestsRidden {
    pub tests: Option<range::Range>,
    pub heart_rates: Arc<Vec<ftp_test::TestHeartRate>>,
}

composed!(
    /// The heart rate each FTP test was ridden at, over the days the tests
    /// span.
    HeartRatesOverTests: TestsRidden = |(tests, heart_rates): (
        Arc<Vec<ftp_test::Test>>,
        Arc<Vec<ftp_test::TestHeartRate>>,
    )| {
        TestsRidden {
            tests: range::Range::spanning(tests.iter().map(ftp_test::Test::on)),
            heart_rates,
        }
    },
    from (Of<FtpTests>, Of<TestHeartRates>)
);

/// The weigh-ins, the composition and the aims, as `/body` reads them
/// together.
pub struct WeightAndComposition {
    pub series: Arc<body_weight::Series>,
    pub components: Arc<Vec<ComponentSeries>>,
    pub aims: Arc<Vec<Aim>>,
}

composed!(
    /// The weigh-ins, the composition and the aims, read together.
    Body: WeightAndComposition = |(series, components, aims): (
        Arc<body_weight::Series>,
        Arc<Vec<ComponentSeries>>,
        Arc<Vec<Aim>>,
    )| {
        WeightAndComposition {
            series,
            components,
            aims,
        }
    },
    from (Of<BodyWeight>, Of<Composition>, Of<Aims>)
);

/// The store, read through what `I` declares: what an analysis that builds
/// its ports as it derives reads (#830).
pub struct Over<I> {
    store: Store,
    zone: OperatorZone,
    reads: std::marker::PhantomData<fn() -> I>,
}

impl<I> Over<I> {
    fn new(store: &Store, zone: &OperatorZone) -> Self {
        Self {
            store: store.clone(),
            zone: zone.clone(),
            reads: std::marker::PhantomData,
        }
    }
}

impl<I: application::graph::Input> application::graph::Input for Over<I> {
    fn reads() -> application::graph::Reads {
        I::reads()
    }
}

/// What Plan reads, as a type.
pub struct PlanReads;

impl application::graph::Input for PlanReads {
    fn reads() -> application::graph::Reads {
        crate::macrocycle::reads()
    }
}

/// Where the plan stands now: the macrocycle's week and its curves, the
/// microcycle's sessions, and what the walk read to say so.
pub struct Standing;

impl Analysed for Standing {
    const ANALYSIS: Analysis = Analysis::Standing;
    type Ports = Over<PlanReads>;
    type Reads = Now;
    type Value = Stood;

    async fn open(store: &Store, zone: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(Over::new(store, zone))
    }

    async fn derive(ports: Self::Ports, now: DayPart) -> Result<Self::Value, NextFailure> {
        crate::macrocycle::stood(&ports.store, &ports.zone, now).await
    }
}

/// What prescribing either discipline reads.
pub struct Prescribers;

impl application::graph::Input for Prescribers {
    fn reads() -> application::graph::Reads {
        <crate::prescribing::Prescriber as application::graph::Input>::reads()
            .and(<crate::cycling::Prescriber<'static> as application::graph::Input>::reads())
    }
}

/// What is next, and the session derived for it, written nowhere: what the
/// home page shows, and what a run issues and delivers.
pub struct Prescription;

impl Analysed for Prescription {
    const ANALYSIS: Analysis = Analysis::Prescription;
    type Ports = Over<Prescribers>;
    type Reads = Of<Standing>;
    type Value = Upcoming;

    async fn open(store: &Store, zone: &OperatorZone) -> Result<Self::Ports, NextFailure> {
        Ok(Over::new(store, zone))
    }

    async fn derive(ports: Self::Ports, stood: Arc<Stood>) -> Result<Self::Value, NextFailure> {
        crate::next::upcoming(&ports.store, &ports.zone, &stood).await
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
        derivation::<Training>(),
        derivation::<Stress>(),
        derivation::<RestingHeartRateResponse>(),
        derivation::<OvernightHrvResponse>(),
        derivation::<StressResponse>(),
        derivation::<OvernightHrv>(),
        derivation::<RestingHeartRate>(),
        derivation::<Sleep>(),
        derivation::<PulseWaveVelocity>(),
        derivation::<Standing>(),
        derivation::<Prescription>(),
        derivation::<Macrocycle>(),
        derivation::<Ranges>(),
        derivation::<Aims>(),
        derivation::<FtpSinceEntry>(),
        derivation::<AtPowerSinceStart>(),
        derivation::<BodyWeightAgainstAims>(),
        derivation::<FatSinceStart>(),
        derivation::<FatFreeSinceStart>(),
        derivation::<GymPerformance>(),
        derivation::<Responses>(),
        derivation::<FitnessSoFar>(),
        derivation::<HeartRatesOverTests>(),
        derivation::<Body>(),
    ]
}
