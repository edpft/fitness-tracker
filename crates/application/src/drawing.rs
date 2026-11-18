//! What a page's drawing was drawn from, and so whether the one kept is still
//! the page (#805).
//!
//! **Read from the ledger and the revisions, never from the layers.** A derived
//! layer stands as it was derived from its ledger entry, and a node nothing
//! derives as it was at its revision, so a drawing whose entries all read the
//! same is the drawing the page would make again.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    sync::{Mutex, PoisonError},
};

use domain::schedule::DayPart;
use sha2::{Digest as _, Sha256};

use crate::{
    StoreError,
    graph::{Graph, Node, Reads},
    ports::{DerivationLedger, Part, Revisions},
};

/// What a drawing was drawn from, fingerprinted: the build that drew it, and
/// where everything its page reads stood.
///
/// **A digest, compared and nothing else**, as
/// [`RenderingDigest`](domain::prescription::RenderingDigest) is: what it was
/// a digest of is said again on the next load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DrawnFrom([u8; 32]);

impl DrawnFrom {
    /// The digest of `stated`, which says where everything read stood.
    #[must_use]
    pub fn of(stated: &str) -> Self {
        Self(Sha256::digest(stated.as_bytes()).into())
    }
}

impl std::fmt::Display for DrawnFrom {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0
            .iter()
            .try_for_each(|byte| write!(formatter, "{byte:02x}"))
    }
}

impl From<[u8; 32]> for DrawnFrom {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl TryFrom<&[u8]> for DrawnFrom {
    type Error = std::array::TryFromSliceError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        <[u8; 32]>::try_from(bytes).map(Self)
    }
}

impl AsRef<[u8]> for DrawnFrom {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Where what a page reads stands, over the ledger and the revisions.
///
/// **Each node is asked once for as long as this lives** (#830): one is made
/// for a page load, and the cards on it and the analyses they read share what
/// it was told.
pub struct Drawing<L, R> {
    ledger: L,
    revisions: R,
    asked: Mutex<BTreeMap<Node, Option<String>>>,
}

impl<L, R> Drawing<L, R>
where
    L: DerivationLedger + Sync,
    R: Revisions + Sync,
{
    pub const fn new(ledger: L, revisions: R) -> Self {
        Self {
            ledger,
            revisions,
            asked: Mutex::new(BTreeMap::new()),
        }
    }

    /// What a page reading `reads`, drawn by `build` at `now`, is drawn from:
    /// `None` where something it reads has no version, which is a page drawn
    /// every time it is opened.
    ///
    /// **A table stands where the streams writing it do**, as the graph says
    /// which they are. **A layer the ledger knows nothing of has no version**:
    /// one derived by hand records none, and one emptied has forgotten it.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn drawn_from(
        &self,
        graph: &Graph,
        reads: &Reads,
        build: &str,
        now: DayPart,
    ) -> Result<Option<DrawnFrom>, StoreError> {
        let mut stated = format!("{build}\n");
        for node in reads.nodes() {
            let Some(stands) = self.stands(graph, node, now).await? else {
                return Ok(None);
            };
            let _infallible = writeln!(stated, "{node:?} {stands}");
        }
        Ok(Some(DrawnFrom::of(&stated)))
    }

    /// Where `node` stands, as text compared and nothing else, or `None` where
    /// it has no version: asked once, and remembered after.
    ///
    /// # Errors
    ///
    /// [`StoreError`] if the store is unavailable.
    pub async fn stands(
        &self,
        graph: &Graph,
        node: &Node,
        now: DayPart,
    ) -> Result<Option<String>, StoreError> {
        if let Some(stands) = self
            .asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(node)
        {
            return Ok(stands.clone());
        }
        let stands = Box::pin(self.asked_now(graph, node, now)).await?;
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(node.clone(), stands.clone());
        Ok(stands)
    }

    /// Where `node` stands, asked of the store.
    ///
    /// **An analysis stands where everything it reads does**, as the graph
    /// says what that is (#830).
    async fn asked_now(
        &self,
        graph: &Graph,
        node: &Node,
        now: DayPart,
    ) -> Result<Option<String>, StoreError> {
        if unversioned(graph, node) {
            return Ok(None);
        }
        Ok(match node {
            Node::Analysis(_) => {
                let mut stated = String::new();
                for read in graph.reads(node).into_iter().flat_map(Reads::nodes) {
                    let Some(stands) = self.stands(graph, read, now).await? else {
                        return Ok(None);
                    };
                    let _infallible = write!(stated, "{read:?}: {stands}; ");
                }
                Some(stated)
            }
            Node::Normalised(stream) => match self.ledger.normalised(stream).await? {
                Some(from) => Some(from.as_str().to_owned()),
                None => self.unrecorded(graph, node).await?,
            },
            Node::Canonical(layer) => match self.ledger.canonical(*layer).await? {
                Some(from) => Some(from.as_str().to_owned()),
                None => self.unrecorded(graph, node).await?,
            },
            Node::Table(_) => {
                let mut stated = String::new();
                for writer in graph.reads(node).into_iter().flat_map(Reads::nodes) {
                    let Node::Normalised(_) = writer else {
                        continue;
                    };
                    let Some(stands) = self.stands(graph, writer, now).await? else {
                        return Ok(None);
                    };
                    let _infallible = write!(stated, "{writer:?}: {stands}; ");
                }
                Some(stated)
            }
            Node::Authored(_) | Node::Generation(_) => {
                Some(self.revisions.revision(node).await?.value().to_string())
            }
            Node::Date => Some(now.date.to_string()),
            Node::DayPart => Some(format!("{now:?}")),
            Node::Landing(_) | Node::Edits(_) | Node::Interpretive(_) => None,
        })
    }
}

impl<L, R> Drawing<L, R>
where
    L: DerivationLedger + Sync,
    R: Revisions + Sync,
{
    /// Where a derived layer that recorded no derivation stands: never
    /// derived, where no stream it is derived from ever was normalised, and
    /// with no version otherwise (#830).
    ///
    /// **A layer never derived is empty**, and any derivation of it, by a run
    /// or by hand, leaves a run behind that moves it. One derived by hand,
    /// which records nothing, still has no version.
    async fn unrecorded(&self, graph: &Graph, node: &Node) -> Result<Option<String>, StoreError> {
        for part in graph.parts(&Reads::of([node.clone()])) {
            if let Part::Normalising(stream) = part
                && self.ledger.ever_normalised(&stream).await?
            {
                return Ok(None);
            }
        }
        Ok(Some(NEVER.to_owned()))
    }
}

/// Where a layer never derived stands.
const NEVER: &str = "never derived";

/// Whether `node` has no version whatever the ledger says.
///
/// **A node read by a derivation and never by a page, or a table not written
/// by normalising alone**: a page reading one is drawn every time it is
/// opened rather than ever served stale.
#[must_use]
pub fn unversioned(graph: &Graph, node: &Node) -> bool {
    match node {
        // **An analysis nothing declares has no version**, and one reading
        // anything unversioned has none either.
        Node::Analysis(_) => graph
            .reads(node)
            .is_none_or(|reads| reads.nodes().any(|read| unversioned(graph, read))),
        Node::Landing(_) | Node::Edits(_) | Node::Interpretive(_) => true,
        Node::Table(_) => graph.reads(node).is_none_or(|writers| {
            writers
                .nodes()
                .any(|writer| !matches!(writer, Node::Normalised(_)))
        }),
        Node::Normalised(_)
        | Node::Canonical(_)
        | Node::Authored(_)
        | Node::Generation(_)
        | Node::Date
        | Node::DayPart => false,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use domain::{
        landing::LandingStream,
        normalised::NormalisationRunId,
        schedule::{DayPart, PartOfDay},
    };

    use super::Drawing;
    use crate::{
        StoreError,
        graph::{Derivation, Graph, Node, Reads, Table},
        ports::{CanonicalLayer, DerivationLedger, DerivedFrom, Revision, Revisions},
    };

    #[derive(Default, Clone)]
    struct Ledger {
        normalised: Arc<Mutex<BTreeMap<LandingStream, String>>>,
        canonical: Arc<Mutex<BTreeMap<CanonicalLayer, String>>>,
        /// Each stream normalised by hand, which records nothing.
        by_hand: Arc<Mutex<std::collections::BTreeSet<LandingStream>>>,
    }

    impl Ledger {
        fn by_hand(&self, stream: LandingStream) {
            self.by_hand.lock().expect("the ledger").insert(stream);
        }
    }

    impl DerivationLedger for Ledger {
        async fn normalised(
            &self,
            stream: &LandingStream,
        ) -> Result<Option<DerivedFrom>, StoreError> {
            Ok(self
                .normalised
                .lock()
                .expect("the ledger")
                .get(stream)
                .cloned()
                .map(DerivedFrom::from))
        }

        async fn ever_normalised(&self, stream: &LandingStream) -> Result<bool, StoreError> {
            Ok(self
                .normalised
                .lock()
                .expect("the ledger")
                .contains_key(stream)
                || self.by_hand.lock().expect("the ledger").contains(stream))
        }

        async fn record_normalised(
            &self,
            _run: NormalisationRunId,
            _from: &DerivedFrom,
        ) -> Result<(), StoreError> {
            Ok(())
        }

        async fn canonical(
            &self,
            layer: CanonicalLayer,
        ) -> Result<Option<DerivedFrom>, StoreError> {
            Ok(self
                .canonical
                .lock()
                .expect("the ledger")
                .get(&layer)
                .cloned()
                .map(DerivedFrom::from))
        }

        async fn record_canonical(
            &self,
            layer: CanonicalLayer,
            from: &DerivedFrom,
        ) -> Result<(), StoreError> {
            self.canonical
                .lock()
                .expect("the ledger")
                .insert(layer, from.as_str().to_owned());
            Ok(())
        }
    }

    #[derive(Default, Clone)]
    struct Written(Arc<Mutex<BTreeMap<Node, u64>>>);

    /// The ledger and the revisions, and a drawing over them for each load,
    /// as a page load makes one.
    #[derive(Default)]
    struct Stored {
        ledger: Ledger,
        revisions: Written,
    }

    impl Stored {
        fn drawing(&self) -> Drawing<Ledger, Written> {
            Drawing::new(self.ledger.clone(), self.revisions.clone())
        }
    }

    impl Written {
        fn write(&self, node: &Node) {
            *self
                .0
                .lock()
                .expect("the revisions")
                .entry(node.clone())
                .or_default() += 1;
        }
    }

    impl Revisions for Written {
        async fn revision(&self, node: &Node) -> Result<Revision, StoreError> {
            Ok(Revision::from(
                self.0
                    .lock()
                    .expect("the revisions")
                    .get(node)
                    .copied()
                    .unwrap_or_default(),
            ))
        }
    }

    fn stream(name: &str) -> LandingStream {
        LandingStream::try_from(name).expect("a stream")
    }

    fn on(date: &str, part: PartOfDay) -> DayPart {
        DayPart::new(date.parse().expect("a date"), part)
    }

    fn run<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime")
            .block_on(future)
    }

    const MORNING: PartOfDay = PartOfDay::Morning;

    /// **A drawing moves with what its page reads, and with nothing else.**
    #[test]
    fn a_drawing_moves_only_with_what_its_page_reads() {
        let stored = Stored::default();
        let graph = Graph::default();
        let reads = Reads::of([Node::Canonical(CanonicalLayer::Gym), Node::PLAN, Node::Date]);
        let now = on("2026-11-14", MORNING);
        let from = || run(stored.drawing().drawn_from(&graph, &reads, "build", now));
        run(stored
            .ledger
            .record_canonical(CanonicalLayer::Gym, &DerivedFrom::from("1".to_owned())))
        .expect("recorded");
        let before = from().expect("read").expect("versioned");

        stored.revisions.write(&Node::DIARY);
        run(stored
            .ledger
            .record_canonical(CanonicalLayer::Cycling, &DerivedFrom::from("1".to_owned())))
        .expect("recorded");
        assert_eq!(
            from().expect("read"),
            Some(before),
            "nothing it reads moved"
        );

        stored.revisions.write(&Node::PLAN);
        let planned = from().expect("read").expect("versioned");
        assert_ne!(planned, before, "the plan was written");

        run(stored
            .ledger
            .record_canonical(CanonicalLayer::Gym, &DerivedFrom::from("2".to_owned())))
        .expect("recorded");
        let derived = from().expect("read").expect("versioned");
        assert_ne!(derived, planned, "the gym layer was derived again");

        let tomorrow =
            run(stored
                .drawing()
                .drawn_from(&graph, &reads, "build", on("2026-11-15", MORNING)))
            .expect("read");
        assert_ne!(tomorrow, Some(derived), "a day passed");

        let rebuilt = run(stored
            .drawing()
            .drawn_from(&graph, &reads, "another build", now))
        .expect("read");
        assert_ne!(rebuilt, Some(derived), "another build draws it differently");
    }

    /// **A page reading only the date is not moved by the part of the day**,
    /// and one reading the part of the day is.
    #[test]
    fn the_part_of_the_day_moves_only_a_page_reading_it() {
        let stored = Stored::default();
        let graph = Graph::default();
        let at = |reads: &Reads, part| {
            run(stored
                .drawing()
                .drawn_from(&graph, reads, "build", on("2026-11-14", part)))
            .expect("read")
        };
        let dated = Reads::of([Node::Date]);
        assert_eq!(at(&dated, MORNING), at(&dated, PartOfDay::Evening));
        let parted = Reads::of([Node::DayPart]);
        assert_ne!(at(&parted, MORNING), at(&parted, PartOfDay::Evening));
    }

    struct Writer;
    impl crate::graph::Input for Writer {
        fn reads() -> Reads {
            Reads::default()
        }
    }
    impl crate::graph::Output for Writer {
        fn writes() -> std::collections::BTreeSet<Table> {
            [Table::named("gym_session")].into()
        }
    }

    /// **A table stands where every stream writing it does**, and has no
    /// version while one of them has none: one normalised by hand, which
    /// records nothing. One never normalised is empty, which is a version.
    #[test]
    fn a_table_stands_where_its_writers_do() {
        let stored = Stored::default();
        let graph = Graph::of(
            ["hevy.workouts", "btwb.exports"]
                .into_iter()
                .flat_map(|name| Derivation::writing::<Writer>(Node::Normalised(stream(name)))),
        );
        let reads = Reads::of([Node::Table(Table::named("gym_session"))]);
        let now = on("2026-11-14", MORNING);
        let from = || run(stored.drawing().drawn_from(&graph, &reads, "build", now)).expect("read");
        let ledger = |name: &str, from: &str| {
            stored
                .ledger
                .normalised
                .lock()
                .expect("the ledger")
                .insert(stream(name), from.to_owned());
        };

        assert_eq!(
            run(stored
                .drawing()
                .drawn_from(&Graph::default(), &reads, "build", now))
            .expect("read"),
            None,
            "a table nothing writes has no version"
        );
        ledger("hevy.workouts", "1");
        let never = from().expect("BTWB's layer was never derived");
        stored.ledger.by_hand(stream("btwb.exports"));
        assert_eq!(from(), None, "BTWB's layer was derived by hand");
        ledger("btwb.exports", "1");
        assert_ne!(from(), Some(never), "BTWB's layer was derived");
        let before = from().expect("versioned");
        ledger("btwb.exports", "2");
        assert_ne!(from(), Some(before), "BTWB's layer was derived again");
    }

    /// **A node only a derivation reads leaves a page drawn every time**,
    /// rather than served as it was.
    #[test]
    fn a_page_reading_an_unversioned_node_is_always_drawn() {
        let stored = Stored::default();
        let reads = Reads::of([Node::Landing(stream("hevy.workouts"))]);
        assert_eq!(
            run(stored.drawing().drawn_from(
                &Graph::default(),
                &reads,
                "build",
                on("2026-11-14", MORNING)
            ))
            .expect("read"),
            None
        );
    }

    struct Weighed;
    impl crate::graph::Input for Weighed {
        fn reads() -> Reads {
            Reads::of([Node::Canonical(CanonicalLayer::WeighIns), Node::Date])
        }
    }

    /// **An analysis stands where everything it reads does** (#830): a card
    /// reading it moves when a layer under it is derived, and again when it
    /// is derived again, and a node asked once in a load is not asked again
    /// in it.
    #[test]
    fn an_analysis_stands_where_what_it_reads_does() {
        use crate::graph::Analysis;
        let stored = Stored::default();
        let graph = Graph::of([Derivation::of::<Weighed>(Node::Analysis(
            Analysis::BodyWeight,
        ))]);
        let reads = Reads::of([Node::Analysis(Analysis::BodyWeight)]);
        let now = on("2026-11-14", MORNING);
        let record = |from: &str| {
            run(stored.ledger.record_canonical(
                CanonicalLayer::WeighIns,
                &DerivedFrom::from(from.to_owned()),
            ))
            .expect("recorded");
        };
        let never = run(stored.drawing().drawn_from(&graph, &reads, "build", now))
            .expect("read")
            .expect("the weigh-ins were never derived, which is a version");
        record("1");
        let load = stored.drawing();
        let before = run(load.drawn_from(&graph, &reads, "build", now))
            .expect("read")
            .expect("versioned");
        assert_ne!(before, never, "the weigh-ins were derived");
        record("2");
        assert_eq!(
            run(load.drawn_from(&graph, &reads, "build", now)).expect("read"),
            Some(before),
            "asked once in a load"
        );
        assert_ne!(
            run(stored.drawing().drawn_from(&graph, &reads, "build", now)).expect("read"),
            Some(before),
            "the weigh-ins were derived again"
        );
    }
}
