//! What a page's drawing was drawn from, and so whether the one kept is still
//! the page (#805).
//!
//! **Read from the ledger and the revisions, never from the layers.** A derived
//! layer stands as it was derived from its ledger entry, and a node nothing
//! derives as it was at its revision, so a drawing whose entries all read the
//! same is the drawing the page would make again.

use std::fmt::Write as _;

use domain::schedule::DayPart;
use sha2::{Digest as _, Sha256};

use crate::{
    StoreError,
    graph::{Graph, Node, Reads},
    ports::{DerivationLedger, Revisions},
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
    fn of(stated: &str) -> Self {
        Self(Sha256::digest(stated.as_bytes()).into())
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
pub struct Drawing<L, R> {
    ledger: L,
    revisions: R,
}

impl<L, R> Drawing<L, R>
where
    L: DerivationLedger + Sync,
    R: Revisions + Sync,
{
    pub const fn new(ledger: L, revisions: R) -> Self {
        Self { ledger, revisions }
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
    /// it has no version.
    async fn stands(
        &self,
        graph: &Graph,
        node: &Node,
        now: DayPart,
    ) -> Result<Option<String>, StoreError> {
        if unversioned(graph, node) {
            return Ok(None);
        }
        Ok(match node {
            Node::Normalised(stream) => self
                .ledger
                .normalised(stream)
                .await?
                .map(|from| from.as_str().to_owned()),
            Node::Canonical(layer) => self
                .ledger
                .canonical(*layer)
                .await?
                .map(|from| from.as_str().to_owned()),
            Node::Table(_) => {
                let mut stated = String::new();
                for writer in graph.reads(node).into_iter().flat_map(Reads::nodes) {
                    let Node::Normalised(stream) = writer else {
                        continue;
                    };
                    let Some(from) = self.ledger.normalised(stream).await? else {
                        return Ok(None);
                    };
                    let _infallible = write!(stated, "{stream}: {}; ", from.as_str());
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

/// Whether `node` has no version whatever the ledger says.
///
/// **A node read by a derivation and never by a page, or a table not written
/// by normalising alone**: a page reading one is drawn every time it is
/// opened rather than ever served stale.
#[must_use]
pub fn unversioned(graph: &Graph, node: &Node) -> bool {
    match node {
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
    use std::{collections::BTreeMap, sync::Mutex};

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

    #[derive(Default)]
    struct Ledger {
        normalised: Mutex<BTreeMap<LandingStream, String>>,
        canonical: Mutex<BTreeMap<CanonicalLayer, String>>,
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

    #[derive(Default)]
    struct Written(Mutex<BTreeMap<Node, u64>>);

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
        let drawing = Drawing::new(Ledger::default(), Written::default());
        let graph = Graph::default();
        let reads = Reads::of([Node::Canonical(CanonicalLayer::Gym), Node::PLAN, Node::Date]);
        let now = on("2026-11-14", MORNING);
        let from = || run(drawing.drawn_from(&graph, &reads, "build", now));
        run(drawing
            .ledger
            .record_canonical(CanonicalLayer::Gym, &DerivedFrom::from("1".to_owned())))
        .expect("recorded");
        let before = from().expect("read").expect("versioned");

        drawing.revisions.write(&Node::DIARY);
        run(drawing
            .ledger
            .record_canonical(CanonicalLayer::Cycling, &DerivedFrom::from("1".to_owned())))
        .expect("recorded");
        assert_eq!(
            from().expect("read"),
            Some(before),
            "nothing it reads moved"
        );

        drawing.revisions.write(&Node::PLAN);
        let planned = from().expect("read").expect("versioned");
        assert_ne!(planned, before, "the plan was written");

        run(drawing
            .ledger
            .record_canonical(CanonicalLayer::Gym, &DerivedFrom::from("2".to_owned())))
        .expect("recorded");
        let derived = from().expect("read").expect("versioned");
        assert_ne!(derived, planned, "the gym layer was derived again");

        let tomorrow = run(drawing.drawn_from(&graph, &reads, "build", on("2026-11-15", MORNING)))
            .expect("read");
        assert_ne!(tomorrow, Some(derived), "a day passed");

        let rebuilt = run(drawing.drawn_from(&graph, &reads, "another build", now)).expect("read");
        assert_ne!(rebuilt, Some(derived), "another build draws it differently");
    }

    /// **A page reading only the date is not moved by the part of the day**,
    /// and one reading the part of the day is.
    #[test]
    fn the_part_of_the_day_moves_only_a_page_reading_it() {
        let drawing = Drawing::new(Ledger::default(), Written::default());
        let graph = Graph::default();
        let at = |reads: &Reads, part| {
            run(drawing.drawn_from(&graph, reads, "build", on("2026-11-14", part))).expect("read")
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
    /// version while one of them has none.
    #[test]
    fn a_table_stands_where_its_writers_do() {
        let drawing = Drawing::new(Ledger::default(), Written::default());
        let graph = Graph::of(
            ["hevy.workouts", "btwb.exports"]
                .into_iter()
                .flat_map(|name| Derivation::writing::<Writer>(Node::Normalised(stream(name)))),
        );
        let reads = Reads::of([Node::Table(Table::named("gym_session"))]);
        let now = on("2026-11-14", MORNING);
        let from = || run(drawing.drawn_from(&graph, &reads, "build", now)).expect("read");
        let ledger = |name: &str, from: &str| {
            drawing
                .ledger
                .normalised
                .lock()
                .expect("the ledger")
                .insert(stream(name), from.to_owned());
        };

        assert_eq!(
            run(drawing.drawn_from(&Graph::default(), &reads, "build", now)).expect("read"),
            None,
            "a table nothing writes has no version"
        );
        ledger("hevy.workouts", "1");
        assert_eq!(from(), None, "BTWB's layer has no version");
        ledger("btwb.exports", "1");
        let before = from().expect("versioned");
        ledger("btwb.exports", "2");
        assert_ne!(from(), Some(before), "BTWB's layer was derived again");
    }

    /// **A node only a derivation reads leaves a page drawn every time**,
    /// rather than served as it was.
    #[test]
    fn a_page_reading_an_unversioned_node_is_always_drawn() {
        let drawing = Drawing::new(Ledger::default(), Written::default());
        let reads = Reads::of([Node::Landing(stream("hevy.workouts"))]);
        assert_eq!(
            run(drawing.drawn_from(
                &Graph::default(),
                &reads,
                "build",
                on("2026-11-14", MORNING)
            ))
            .expect("read"),
            None
        );
    }
}
