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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reads(BTreeSet<Node>);

impl Reads {
    #[must_use]
    pub fn of(nodes: impl IntoIterator<Item = Node>) -> Self {
        Self(nodes.into_iter().collect())
    }

    /// What either reads.
    #[must_use]
    pub fn and(mut self, other: Self) -> Self {
        self.0.extend(other.0);
        self
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.0.iter()
    }

    #[must_use]
    pub fn has(&self, node: &Node) -> bool {
        self.0.contains(node)
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
    /// inputs nothing derives.
    #[must_use]
    pub fn upstream(&self, reads: &Reads) -> BTreeSet<Node> {
        let mut seen = BTreeSet::new();
        let mut pending: Vec<&Node> = reads.nodes().collect();
        while let Some(node) = pending.pop() {
            if seen.insert(node.clone())
                && let Some(inputs) = self.derivations.get(node)
            {
                pending.extend(inputs.nodes());
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
                | Node::Interpretive(_) => None,
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
