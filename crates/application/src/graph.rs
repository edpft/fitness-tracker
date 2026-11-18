//! What the tool holds, and what reads it (#809).
//!
//! **Each node is read through a port, and the port says which.** A use case
//! is generic over its ports and has no other way into the store, so what it
//! reads is the union of what its ports read: [`Input::reads`] is computed from
//! the types it is given, never written beside them. The graph is every
//! derivation's reads together, as dbt's is every model's `ref`s.

use std::collections::{BTreeMap, BTreeSet};

use domain::landing::LandingStream;

use crate::ports::{CanonicalLayer, Part};

/// Anything that can change, and that something reads.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Node {
    // § II, what happened: observations, layered.
    Landing(LandingStream),
    Normalised(LandingStream),
    Canonical(CanonicalLayer),
    /// A table of the normalised layer, which several streams may write: it
    /// is derived from every stream whose layer writes it.
    Table(Table),
    /// The edit overlay, an input to that stream's normalised layer.
    Edits(LandingStream),
    // § III, what we intend and what we consult: not layered.
    /// § 12.
    Authored(Authored),
    /// § 13.
    Interpretive(Interpretive),
    /// § 14.
    Generation(Generation),
    // What changes with no write at all (#805).
    /// Today's date, which a page measured "to today" reads: a day passing
    /// changes it with nothing new in the store.
    Date,
    /// The part of the day now, which where the plan stands is judged at.
    DayPart,
    // What is derived from the store and held in memory, never written (#830).
    Analysis(Analysis),
}

/// What is read from the store and worked out from it, held in memory while
/// nothing it reads has moved, and shared by every card reading it (#830).
///
/// **Not in the store**: what one stands at is where everything upstream of it
/// does, so a card reading one is drawn again when any of that moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Analysis {
    /// Every canonical weigh-in, and the trend through them.
    BodyWeight,
    /// Every component anything measured, one series per method.
    Composition,
    /// The plans, as written and as they now stand.
    Plans,
    /// Which device sent each reading.
    Register,
    /// The functional threshold power, as it has been tested.
    FtpHistory,
    /// Every FTP test, in watts and over body weight.
    FtpTests,
    /// The heart rate each FTP test was ridden at.
    TestHeartRates,
    /// Each main ride's heart rate.
    RideHeartRates,
    /// Heart rate at a fixed power, one line per device.
    HeartRateAtPower,
    /// Each lift's 1RM tests and its estimates over the macrocycle.
    Strength,
    /// The training each day carries, through today.
    Training,
    /// Each day's stress, one series per watch.
    Stress,
    /// Resting heart rate against the training done, over the macrocycle.
    RestingHeartRateResponse,
    /// Overnight HRV against the training done, over the macrocycle.
    OvernightHrvResponse,
    /// Stress against the training done, over the macrocycle.
    StressResponse,
    /// Each night's HRV, one series per watch.
    OvernightHrv,
    /// Each day's resting heart rate, one series per watch.
    RestingHeartRate,
    /// Each night's sleep, one series per watch.
    Sleep,
    /// Pulse wave velocity, one series per algorithm.
    PulseWaveVelocity,
    /// Where the plan stands now: the macrocycle's week and its curves, and
    /// the microcycle's sessions.
    Standing,
    /// What is next, and the session derived for it, issued or not.
    Prescription,
    /// This microcycle's sessions.
    Microcycle,
    /// The macrocycle's week, and its volume and intensity curves.
    Curves,
    /// The macrocycle in force today, where one is.
    Macrocycle,
    /// The ranges a chart can be read over today.
    Ranges,
    /// The plan's aims for body weight, as they stand today.
    Aims,
    /// The latest FTP test against the macrocycle's entry test.
    FtpSinceEntry,
    /// Heart rate at 140 W since the macrocycle's start.
    AtPowerSinceStart,
    /// Body weight's trend since the macrocycle's start, against the aims.
    BodyWeightAgainstAims,
    /// Fat mass since the macrocycle's start.
    FatSinceStart,
    /// Fat-free mass since the macrocycle's start.
    FatFreeSinceStart,
    /// Each lift's 1RM test against its entry test, and its estimate since
    /// the start.
    GymPerformance,
    /// Each response reading over the macrocycle, read together.
    Responses,
    /// Whether the macrocycle has improved his fitness so far: every test
    /// and every response reading.
    FitnessSoFar,
    /// The heart rate each FTP test was ridden at, over the days the tests
    /// span.
    HeartRatesOverTests,
    /// The weigh-ins, the composition and the aims, read together.
    Body,
}

impl Analysis {
    pub const ALL: [Self; 36] = [
        Self::BodyWeight,
        Self::Composition,
        Self::Plans,
        Self::Register,
        Self::FtpHistory,
        Self::FtpTests,
        Self::TestHeartRates,
        Self::RideHeartRates,
        Self::HeartRateAtPower,
        Self::Strength,
        Self::Training,
        Self::Stress,
        Self::RestingHeartRateResponse,
        Self::OvernightHrvResponse,
        Self::StressResponse,
        Self::OvernightHrv,
        Self::RestingHeartRate,
        Self::Sleep,
        Self::PulseWaveVelocity,
        Self::Standing,
        Self::Prescription,
        Self::Microcycle,
        Self::Curves,
        Self::Macrocycle,
        Self::Ranges,
        Self::Aims,
        Self::FtpSinceEntry,
        Self::AtPowerSinceStart,
        Self::BodyWeightAgainstAims,
        Self::FatSinceStart,
        Self::FatFreeSinceStart,
        Self::GymPerformance,
        Self::Responses,
        Self::FitnessSoFar,
        Self::HeartRatesOverTests,
        Self::Body,
    ];

    /// Its name, as telemetry records it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BodyWeight => "body weight",
            Self::Composition => "composition",
            Self::Plans => "plans",
            Self::Register => "register",
            Self::FtpHistory => "ftp history",
            Self::FtpTests => "ftp tests",
            Self::TestHeartRates => "test heart rates",
            Self::RideHeartRates => "ride heart rates",
            Self::HeartRateAtPower => "heart rate at power",
            Self::Strength => "strength",
            Self::Training => "training",
            Self::Stress => "stress",
            Self::RestingHeartRateResponse => "resting heart rate response",
            Self::OvernightHrvResponse => "overnight hrv response",
            Self::StressResponse => "stress response",
            Self::OvernightHrv => "overnight hrv",
            Self::RestingHeartRate => "resting heart rate",
            Self::Sleep => "sleep",
            Self::PulseWaveVelocity => "pulse wave velocity",
            Self::Standing => "standing",
            Self::Prescription => "prescription",
            Self::Microcycle => "microcycle",
            Self::Curves => "curves",
            Self::Macrocycle => "macrocycle",
            Self::Ranges => "ranges",
            Self::Aims => "aims",
            Self::FtpSinceEntry => "ftp since entry",
            Self::AtPowerSinceStart => "heart rate at power since start",
            Self::BodyWeightAgainstAims => "body weight against aims",
            Self::FatSinceStart => "fat since start",
            Self::FatFreeSinceStart => "fat-free since start",
            Self::GymPerformance => "gym performance",
            Self::Responses => "responses",
            Self::FitnessSoFar => "fitness so far",
            Self::HeartRatesOverTests => "heart rates over tests",
            Self::Body => "body",
        }
    }
}

/// What is authored, by name, so a driven adapter declaring that it reads one
/// names only the node (#809).
impl Node {
    pub const PLAN: Self = Self::Authored(Authored::Plan);
    pub const PRESCRIPTIONS: Self = Self::Authored(Authored::Prescriptions);
    pub const WORKOUTS: Self = Self::Authored(Authored::Workouts);
    pub const DIARY: Self = Self::Authored(Authored::Diary);
}

/// What the operator authors (§ 12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Authored {
    /// The plans and the mesocycles committed from them.
    Plan,
    /// What was prescribed, and where it was delivered.
    Prescriptions,
    /// How each workout and each primary progression prescribes, as
    /// designed (§ 14.1).
    Workouts,
    /// When there is room to train, and what departs from it.
    Diary,
}

/// What observations are interpreted under (§ 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Interpretive {
    /// The zones stated over time, which place an observation's day.
    Zones,
}

/// What programming consults about the world it is run in (§ 14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Generation {
    /// The plates and fills to hand.
    Equipment,
    /// The classes a ride can be prescribed from, as the catalogue last read
    /// them.
    Classes,
    Calendar(Calendar),
}

/// A calendar the training is run in, as the body that sets it publishes it:
/// received from outside, and kept as it was last served (§ 14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Calendar {
    /// The school's term dates.
    School,
    /// The public holidays.
    Public,
}

impl Calendar {
    pub const ALL: [Self; 2] = [Self::School, Self::Public];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::School => "school",
            Self::Public => "public",
        }
    }
}

/// A table of the normalised layer, by the name its SQL gives it (#809).
///
/// **The join between a stream's layer and what reads it.** A writer names
/// the tables it writes and a reader the tables it reads, each beside its own
/// SQL, so a reader never lists the streams that write its table: a stream
/// that starts writing one is read wherever that table is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Table(&'static str);

impl Table {
    #[must_use]
    pub const fn named(name: &'static str) -> Self {
        Self(name)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// What a derivation, a page or a prescription reads.
///
/// **Two kinds of read** (#830). What it is versioned by moves it: it is
/// derived or drawn again whenever one of those does. What it only consults is
/// read as it stands whenever it is derived, and moves nothing. A test's per
/// kilo is fixed by the weigh-in on the day of the test, so the tests consult
/// body weight: a weigh-in today changes no test, and the tests are derived
/// again only when they move.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reads {
    versions: BTreeSet<Node>,
    consulted: BTreeSet<Node>,
}

impl Reads {
    /// Reads of `nodes` that version what reads them.
    #[must_use]
    pub fn of(nodes: impl IntoIterator<Item = Node>) -> Self {
        Self {
            versions: nodes.into_iter().collect(),
            consulted: BTreeSet::new(),
        }
    }

    /// Reads of `nodes`, consulted as they stand when what reads them is
    /// derived, and moving nothing.
    #[must_use]
    pub fn consulting(nodes: impl IntoIterator<Item = Node>) -> Self {
        Self {
            versions: BTreeSet::new(),
            consulted: nodes.into_iter().collect(),
        }
    }

    /// What either reads.
    #[must_use]
    pub fn and(mut self, other: Self) -> Self {
        self.versions.extend(other.versions);
        self.consulted.extend(other.consulted);
        self
    }

    /// What it is versioned by.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.versions.iter()
    }

    /// What it only consults.
    pub fn consulted(&self) -> impl Iterator<Item = &Node> {
        self.consulted.iter()
    }

    /// Whether it is versioned by `node`.
    #[must_use]
    pub fn has(&self, node: &Node) -> bool {
        self.versions.contains(node)
    }
}

impl FromIterator<Node> for Reads {
    fn from_iter<I: IntoIterator<Item = Node>>(nodes: I) -> Self {
        Self::of(nodes)
    }
}

/// A port, or a use case over ports, and the nodes it reads.
///
/// **Of the type, not of a value**, so what a page reads is known before
/// anything is opened, as a dbt model's refs are known before it runs. A use
/// case implements it as the union over its ports, so it cannot read what it
/// does not declare: it has nothing else to read with.
pub trait Input {
    fn reads() -> Reads;
}

/// A port that writes the normalised layer, and the tables it writes (#809).
///
/// **Named beside the SQL that writes them**, as [`Input`] is beside the SQL
/// that reads. A stream's layer is in the graph only through its writer, so a
/// writer that does not say what it writes is not one a stream can be
/// normalised through.
pub trait Output {
    fn writes() -> BTreeSet<Table>;
}

/// A port that writes a node nothing derives, and which (#805).
///
/// **Named beside the SQL that writes them**, as [`Output`] is. A page drawn
/// from one of these nodes is drawn again when its [`Revision`] moves, so a
/// writer that does not say what it authors is one whose writes no page sees.
///
/// [`Revision`]: crate::ports::Revision
pub trait Authors {
    fn authors() -> BTreeSet<Node>;
}

/// What a port that writes and reads nothing reads.
impl Input for () {
    fn reads() -> Reads {
        Reads::default()
    }
}

macro_rules! tuple_input {
    ($($name:ident),+) => {
        /// What each reads.
        impl<$($name: Input),+> Input for ($($name,)+) {
            fn reads() -> Reads {
                Reads::default()$(.and($name::reads()))+
            }
        }
    };
}

tuple_input!(A);
tuple_input!(A, B);
tuple_input!(A, B, C);
tuple_input!(A, B, C, D);
tuple_input!(A, B, C, D, E);
tuple_input!(A, B, C, D, E, F);
tuple_input!(A, B, C, D, E, F, G);
tuple_input!(A, B, C, D, E, F, G, H);

/// **A use case reads what its ports read**, each port a type parameter:
/// `through!(Sleeping<S, R, W>)` reads what `S`, `R` and `W` do.
macro_rules! through {
    ($($use_case:ident<$($port:ident),+>),+ $(,)?) => {$(
        impl<$($port: $crate::graph::Input),+> $crate::graph::Input for $use_case<$($port),+> {
            fn reads() -> $crate::graph::Reads {
                $crate::graph::Reads::default()$(.and($port::reads()))+
            }
        }
    )+};
}
pub(crate) use through;

/// The estimate of a one-repetition maximum is arithmetic, and reads nothing.
impl Input for domain::analytical::Rts {
    fn reads() -> Reads {
        Reads::default()
    }
}

impl<T: Input> Input for &T {
    fn reads() -> Reads {
        T::reads()
    }
}

impl<T: Input> Input for Option<T> {
    fn reads() -> Reads {
        T::reads()
    }
}

impl<T: Input, E> Input for Result<T, E> {
    fn reads() -> Reads {
        T::reads()
    }
}

/// A derived node, and the only nodes it may read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derivation {
    pub node: Node,
    pub reads: Reads,
}

impl Derivation {
    /// The derivation of `node` by `D`, which reads what its ports read.
    #[must_use]
    pub fn of<D: Input>(node: Node) -> Self {
        Self {
            node,
            reads: D::reads(),
        }
    }

    /// The derivation of `node` by `D`, and of each table `D` writes from
    /// `node` among the others that write it.
    pub fn writing<D: Input + Output>(node: Node) -> impl Iterator<Item = Self> {
        let tables = D::writes().into_iter().map({
            let node = node.clone();
            move |table| Self {
                node: Node::Table(table),
                reads: Reads::of([node.clone()]),
            }
        });
        std::iter::once(Self::of::<D>(node)).chain(tables)
    }
}

/// Every derivation this build makes: the union of their declarations.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    derivations: BTreeMap<Node, Reads>,
}

impl Graph {
    #[must_use]
    pub fn of(derivations: impl IntoIterator<Item = Derivation>) -> Self {
        let mut graph = Self::default();
        for derivation in derivations {
            let reads = graph
                .derivations
                .remove(&derivation.node)
                .unwrap_or_default();
            graph
                .derivations
                .insert(derivation.node, reads.and(derivation.reads));
        }
        graph
    }

    /// What `node` is derived from directly, `None` for a node nothing
    /// derives.
    #[must_use]
    pub fn reads(&self, node: &Node) -> Option<&Reads> {
        self.derivations.get(node)
    }

    /// Every node derived here.
    pub fn derived(&self) -> impl Iterator<Item = &Node> {
        self.derivations.keys()
    }

    /// Everything `reads` reads, and everything that is derived from, to the
    /// inputs nothing derives: what it consults as well as what versions it.
    #[must_use]
    pub fn upstream(&self, reads: &Reads) -> BTreeSet<Node> {
        self.walk(reads, |reads| reads.nodes().chain(reads.consulted()))
    }

    /// What moves what `reads` reads: everything it is versioned by, and
    /// everything those are versioned by, to the inputs nothing derives.
    #[must_use]
    pub fn moved_by(&self, reads: &Reads) -> BTreeSet<Node> {
        self.walk(reads, Reads::nodes)
    }

    /// Every node reached from `reads`, following what `follow` says each
    /// one reads.
    fn walk<'a, I: Iterator<Item = &'a Node>>(
        &'a self,
        reads: &'a Reads,
        follow: impl Fn(&'a Reads) -> I,
    ) -> BTreeSet<Node> {
        let mut seen = BTreeSet::new();
        let mut pending: Vec<&Node> = follow(reads).collect();
        while let Some(node) = pending.pop() {
            if seen.insert(node.clone())
                && let Some(inputs) = self.derivations.get(node)
            {
                pending.extend(follow(inputs));
            }
        }
        seen
    }

    /// The parts a run collecting what `reads` reads takes: each node that is
    /// fetched, asked or derived, and every one it is derived from.
    #[must_use]
    pub fn parts(&self, reads: &Reads) -> BTreeSet<Part> {
        self.upstream(reads)
            .into_iter()
            .filter_map(|node| match node {
                Node::Landing(stream) => Some(Part::Fetching(stream)),
                Node::Normalised(stream) => Some(Part::Normalising(stream)),
                Node::Canonical(layer) => Some(Part::Canonicalising(layer)),
                Node::Generation(Generation::Calendar(calendar)) => {
                    Some(Part::FetchingCalendar(calendar))
                }
                // **Refreshed only by a run that delivers** (#246), which is
                // the only one that prescribes from them.
                // **A table is no step of its own**: its streams' layers are.
                Node::Generation(Generation::Classes | Generation::Equipment)
                | Node::Table(_)
                | Node::Edits(_)
                | Node::Authored(_)
                | Node::Interpretive(_)
                | Node::Date
                | Node::DayPart
                | Node::Analysis(_) => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use domain::landing::LandingStream;

    use super::{Derivation, Graph, Input, Node, Reads};
    use crate::ports::{CanonicalLayer, Part};

    fn stream(name: &str) -> LandingStream {
        LandingStream::try_from(name).expect("a stream")
    }

    struct Rides;
    impl Input for Rides {
        fn reads() -> Reads {
            Reads::of([Node::Landing(stream("peloton.rides"))])
        }
    }

    struct Normalised;
    impl Input for Normalised {
        fn reads() -> Reads {
            Reads::of([Node::Normalised(stream("peloton.rides"))])
        }
    }

    /// **A page reading the canonical rides collects every node they are
    /// derived from**, and nothing derived from something it does not read.
    #[test]
    fn a_read_collects_everything_upstream_of_it() {
        let graph = Graph::of([
            Derivation::of::<Rides>(Node::Normalised(stream("peloton.rides"))),
            Derivation::of::<Normalised>(Node::Canonical(CanonicalLayer::Cycling)),
            Derivation::of::<()>(Node::Normalised(stream("garmin.hrv"))),
        ]);
        assert_eq!(
            graph
                .parts(&Reads::of([Node::Canonical(CanonicalLayer::Cycling)]))
                .into_iter()
                .collect::<Vec<_>>(),
            [
                Part::Fetching(stream("peloton.rides")),
                Part::Normalising(stream("peloton.rides")),
                Part::Canonicalising(CanonicalLayer::Cycling),
            ]
        );
    }

    /// **What a node consults is collected for it, and moves nothing**
    /// (#830): the rides consulting the weigh-ins still wait on Withings to be
    /// fetched, but a weigh-in does not move them.
    #[test]
    fn a_consulted_read_is_collected_and_moves_nothing() {
        let weigh_ins = Node::Normalised(stream("withings.measurements"));
        let graph = Graph::of([Derivation {
            node: Node::Canonical(CanonicalLayer::Cycling),
            reads: Reads::of([Node::Normalised(stream("peloton.rides"))])
                .and(Reads::consulting([weigh_ins.clone()])),
        }]);
        let rides = Reads::of([Node::Canonical(CanonicalLayer::Cycling)]);
        assert!(graph.upstream(&rides).contains(&weigh_ins));
        assert!(!graph.moved_by(&rides).contains(&weigh_ins));
        assert!(
            graph
                .moved_by(&rides)
                .contains(&Node::Normalised(stream("peloton.rides")))
        );
    }

    struct Writer;
    impl Input for Writer {
        fn reads() -> Reads {
            Reads::default()
        }
    }
    impl super::Output for Writer {
        fn writes() -> std::collections::BTreeSet<super::Table> {
            [super::Table::named("gym_session")].into()
        }
    }

    /// **A table is read from every stream that writes it** (#809): a reader
    /// names the table, never its writers, so one writing it later is read
    /// with no change to the reader.
    #[test]
    fn a_table_is_derived_from_every_stream_writing_it() {
        let graph = Graph::of(
            ["hevy.workouts", "btwb.exports"]
                .into_iter()
                .flat_map(|name| Derivation::writing::<Writer>(Node::Normalised(stream(name)))),
        );
        assert_eq!(
            graph
                .parts(&Reads::of([Node::Table(super::Table::named(
                    "gym_session"
                ))]))
                .into_iter()
                .collect::<Vec<_>>(),
            [
                Part::Normalising(stream("btwb.exports")),
                Part::Normalising(stream("hevy.workouts")),
            ]
        );
    }

    /// **A use case reads what its ports read**, as a tuple of them does.
    #[test]
    fn a_tuple_reads_what_each_reads() {
        assert_eq!(
            <(Rides, Normalised)>::reads(),
            Reads::of([
                Node::Landing(stream("peloton.rides")),
                Node::Normalised(stream("peloton.rides")),
            ])
        );
    }
}
