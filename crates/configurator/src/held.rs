//! What is derived from the store, held in memory while nothing it reads has
//! moved (#830).
//!
//! **Each analysis is derived once for each version of what it reads**, and
//! every card reading it, on any page, is handed that one result. Its version
//! is #805's: where the ledger and the revisions say everything upstream of it
//! stands. A write made by another process moves those, so the next load
//! derives it again rather than serving what was derived before the write.
//!
//! **What something reads is a type, and it is handed the values and nothing
//! else**: [`Fetch`] gives up only what its type declares, so a card or an
//! analysis has no way to read what it does not.

use std::{
    any::Any,
    collections::HashMap,
    fmt::Write as _,
    marker::PhantomData,
    pin::Pin,
    sync::{Arc, Mutex, PoisonError},
};

use application::{
    StoreError,
    drawing::{Drawing, DrawnFrom},
    graph::{Analysis, Derivation, Input, Node, Reads},
};
use domain::{normalised::OperatorZone, schedule::DayPart};
use infrastructure::{SqliteDerivationLedger, SqliteRevisions};
use jiff::civil::Date;
use tokio::sync::OnceCell;
use tracing::Instrument as _;

use crate::{
    clock::{Now, Today},
    next::NextFailure,
    store::{Spanned, Store},
};

/// Something worked out from the store, held while nothing it reads moves.
pub trait Analysed: Send + Sync + 'static {
    /// The node it is.
    const ANALYSIS: Analysis;

    /// What it reads the store through.
    type Ports: Input + Send + Sync;

    /// What else it reads: other analyses, and the date.
    type Reads: Fetch;

    /// What it holds.
    type Value: PartialEq + Send + Sync + 'static;

    /// Its ports over `store`.
    fn open(
        store: &Store,
        zone: &OperatorZone,
    ) -> impl Future<Output = Result<Self::Ports, NextFailure>> + Send;

    /// Work it out from what it reads.
    fn derive(
        ports: Self::Ports,
        read: <Self::Reads as Fetch>::Fetched,
    ) -> impl Future<Output = Result<Self::Value, NextFailure>> + Send;
}

/// The derivation of `A`: what its ports and what else it reads.
#[must_use]
pub fn derivation<A: Analysed>() -> Derivation {
    Derivation {
        node: Node::Analysis(A::ANALYSIS),
        reads: <A::Ports as Input>::reads().and(<A::Reads as Input>::reads()),
    }
}

/// What a card or an analysis reads, as a type, and fetched as one.
pub trait Fetch: Input {
    /// What it hands over.
    type Fetched: Send;

    /// Fetch it, from what is held where it still stands.
    fn fetch<'a>(holding: &'a Holding<'a>) -> FetchFuture<'a, Self::Fetched>;
}

/// What [`Fetch::fetch`] returns: boxed, since an analysis fetching another
/// is a recursion.
pub type FetchFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, NextFailure>> + Send + 'a>>;

/// Reading the analysis `A`.
pub struct Of<A>(PhantomData<fn() -> A>);

impl<A: Analysed> Input for Of<A> {
    fn reads() -> Reads {
        Reads::of([Node::Analysis(A::ANALYSIS)])
    }
}

impl<A: Analysed> Fetch for Of<A> {
    type Fetched = Arc<A::Value>;

    fn fetch<'a>(holding: &'a Holding<'a>) -> FetchFuture<'a, Self::Fetched> {
        Box::pin(holding.held::<A>())
    }
}

/// Consulting the analysis `A`: read as it stands whenever what reads it is
/// derived, and moving nothing (#830).
pub struct Consulted<A>(PhantomData<fn() -> A>);

impl<A: Analysed> Input for Consulted<A> {
    fn reads() -> Reads {
        Reads::consulting([Node::Analysis(A::ANALYSIS)])
    }
}

impl<A: Analysed> Fetch for Consulted<A> {
    type Fetched = Arc<A::Value>;

    fn fetch<'a>(holding: &'a Holding<'a>) -> FetchFuture<'a, Self::Fetched> {
        Box::pin(holding.held::<A>())
    }
}

/// What a card reads (#830): one analysis, and the ranges where it offers a
/// choice of one.
///
/// The operator, 2026-11-17: one metric is one analysis,
/// *"which itself takes n analyses as inputs"*. So a card that would read two
/// analyses, the store or the date is a compile error, and a reading shown on
/// two pages is worked out once, by the analysis both read.
///
/// **Sealed**: nothing outside this module can widen it.
pub trait Shown: Fetch + sealed::Sealed {}

mod sealed {
    pub trait Sealed {}
}

impl<A: Analysed> sealed::Sealed for Of<A> {}
impl<A: Analysed> Shown for Of<A> {}

/// Reading the analysis `A` and the ranges a chart can be read over: what a
/// card offering a choice of range reads.
///
/// **The ranges choose the days shown and are no reading**, so every chart
/// reads the one node saying what "this macrocycle" is.
pub struct Ranged<A>(PhantomData<fn() -> A>);

impl<A: Analysed> Input for Ranged<A> {
    fn reads() -> Reads {
        <(Of<A>, Of<crate::analyses::Ranges>) as Input>::reads()
    }
}

impl<A: Analysed> Fetch for Ranged<A> {
    type Fetched = (
        Arc<A::Value>,
        Arc<<crate::analyses::Ranges as Analysed>::Value>,
    );

    fn fetch<'a>(holding: &'a Holding<'a>) -> FetchFuture<'a, Self::Fetched> {
        <(Of<A>, Of<crate::analyses::Ranges>) as Fetch>::fetch(holding)
    }
}

impl<A: Analysed> sealed::Sealed for Ranged<A> {}
impl<A: Analysed> Shown for Ranged<A> {}

impl Fetch for Today {
    type Fetched = Date;

    fn fetch<'a>(holding: &'a Holding<'a>) -> FetchFuture<'a, Self::Fetched> {
        Box::pin(std::future::ready(Ok(holding.now.date)))
    }
}

impl Fetch for Now {
    type Fetched = DayPart;

    fn fetch<'a>(holding: &'a Holding<'a>) -> FetchFuture<'a, Self::Fetched> {
        Box::pin(std::future::ready(Ok(holding.now)))
    }
}

impl Fetch for () {
    type Fetched = ();

    fn fetch<'a>(_: &'a Holding<'a>) -> FetchFuture<'a, Self::Fetched> {
        Box::pin(std::future::ready(Ok(())))
    }
}

macro_rules! tuple_fetch {
    ($($name:ident),+) => {
        /// Each, in turn.
        impl<$($name: Fetch),+> Fetch for ($($name,)+) {
            type Fetched = ($($name::Fetched,)+);

            fn fetch<'a>(holding: &'a Holding<'a>) -> FetchFuture<'a, Self::Fetched> {
                Box::pin(async move { Ok(($($name::fetch(holding).await?,)+)) })
            }
        }
    };
}

tuple_fetch!(A);
tuple_fetch!(A, B);
tuple_fetch!(A, B, C);
tuple_fetch!(A, B, C, D);
tuple_fetch!(A, B, C, D, E);
tuple_fetch!(A, B, C, D, E, F);
tuple_fetch!(A, B, C, D, E, F, G);
tuple_fetch!(A, B, C, D, E, F, G, H);

/// An analysis as it was derived: its value, and its version.
///
/// **Its version is where its inputs stood when its value last changed**: a
/// derivation that comes out as the one before keeps that one's version, so
/// nothing reading it moves (#830).
#[derive(Clone)]
struct Derived {
    version: String,
    value: Arc<dyn Any + Send + Sync>,
}

/// An analysis derived from inputs standing at `inputs`, once it has been.
struct Slot {
    inputs: DrawnFrom,
    derived: Arc<OnceCell<Derived>>,
}

/// Where an analysis held stands against where its inputs now do.
enum Standing {
    /// Derived from them, at this version.
    Derived(String),
    /// Not derived by this server: it stands where its inputs do.
    Never,
    /// Derived from inputs that have moved since, so whether it has moved is
    /// known only once it is derived again.
    Moved,
}

/// Every analysis held, one value each, as it was last derived.
///
/// **One per server**, so two pages, or two cards, reading one analysis are
/// handed one result.
#[derive(Default)]
pub struct Held {
    slots: Mutex<HashMap<Analysis, Slot>>,
}

impl Held {
    /// Where `analysis` stands, its inputs standing at `inputs`.
    fn standing(&self, analysis: Analysis, inputs: DrawnFrom) -> Standing {
        let slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        slots
            .get(&analysis)
            .map_or(Standing::Never, |slot| match slot.derived.get() {
                Some(derived) if slot.inputs == inputs => {
                    Standing::Derived(derived.version.clone())
                }
                _ => Standing::Moved,
            })
    }

    /// The slot for `analysis` derived from `inputs`, and what it held before
    /// where that was derived from anything else.
    fn slot(
        &self,
        analysis: Analysis,
        inputs: DrawnFrom,
    ) -> (Arc<OnceCell<Derived>>, Option<Derived>) {
        let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(slot) = slots.get(&analysis)
            && slot.inputs == inputs
        {
            return (Arc::clone(&slot.derived), None);
        }
        let derived = Arc::new(OnceCell::new());
        let replaced = slots.insert(
            analysis,
            Slot {
                inputs,
                derived: Arc::clone(&derived),
            },
        );
        drop(slots);
        let before = replaced.and_then(|slot| slot.derived.get().cloned());
        (derived, before)
    }
}

/// `value` as derived from inputs standing at `inputs`: at the version of
/// what was held `before` where it is the same value, and at `inputs`
/// otherwise.
fn derived<V: PartialEq + Send + Sync + 'static>(
    value: Arc<V>,
    inputs: DrawnFrom,
    before: Option<&Derived>,
) -> Derived {
    let version = before
        .filter(|before| before.value.downcast_ref::<V>() == Some(&*value))
        .map_or_else(|| inputs.to_string(), |before| before.version.clone());
    Derived { version, value }
}

/// What is held, as one page load reads it: the store, the zone, the build,
/// and when.
///
/// **Made for each load**, so each node's version is asked of the store once
/// in it, however many cards and analyses read the node.
pub struct Holding<'a> {
    held: &'a Held,
    store: &'a Store,
    zone: &'a OperatorZone,
    build: Option<&'a str>,
    now: DayPart,
    drawing: Drawing<Spanned<SqliteDerivationLedger>, Spanned<SqliteRevisions>>,
}

impl<'a> Holding<'a> {
    /// A load of what `held` holds, over `store`, by `build`: `None` where it
    /// is not known, and then nothing is held and everything is derived.
    #[must_use]
    pub fn new(
        held: &'a Held,
        store: &'a Store,
        zone: &'a OperatorZone,
        build: Option<&'a str>,
    ) -> Self {
        Self {
            held,
            store,
            zone,
            build,
            now: Now::new(zone.clone()).day_part(),
            drawing: store.drawing(),
        }
    }

    /// Where everything `reads` reads stands now: `None` where something in
    /// it has no version, or the build is not known, or an analysis in it has
    /// to be derived again to say.
    ///
    /// **A store that cannot say has no version**, and what reads it is
    /// derived again rather than served as it was.
    pub async fn drawn_from(&self, reads: &Reads) -> Option<DrawnFrom> {
        let build = self.build?;
        async {
            let mut stated = format!("{build}\n");
            for node in reads.nodes() {
                let stands = self.version(node).await?;
                let _infallible = writeln!(stated, "{node:?} {stands}");
            }
            Some(DrawnFrom::of(&stated))
        }
        .instrument(crate::telemetry::keying())
        .await
    }

    /// Where `node` stands: an analysis at its version, and anything else as
    /// the store says.
    fn version<'b>(
        &'b self,
        node: &'b Node,
    ) -> Pin<Box<dyn Future<Output = Option<String>> + Send + 'b>> {
        Box::pin(async move {
            let Node::Analysis(analysis) = node else {
                return self
                    .drawing
                    .stands(crate::graph::graph(), node, self.now)
                    .await
                    .ok()
                    .flatten();
            };
            let inputs = self.inputs(node).await?;
            match self.held.standing(*analysis, inputs) {
                Standing::Derived(version) => Some(version),
                Standing::Never => Some(inputs.to_string()),
                Standing::Moved => None,
            }
        })
    }

    /// Where everything the analysis `node` reads stands.
    async fn inputs(&self, node: &Node) -> Option<DrawnFrom> {
        let reads = crate::graph::graph().reads(node)?;
        let mut stated = String::new();
        for read in reads.nodes() {
            let stands = self.version(read).await?;
            let _infallible = writeln!(stated, "{read:?} {stands}");
        }
        Some(DrawnFrom::of(&stated))
    }

    /// Fetch what `F` reads.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn fetch<F: Fetch>(&self) -> Result<F::Fetched, NextFailure> {
        F::fetch(self).await
    }

    /// `A` as held, where it was derived from what its inputs now stand at.
    fn standing<A: Analysed>(&self, inputs: Option<DrawnFrom>) -> Option<Arc<A::Value>> {
        let inputs = inputs?;
        let value = self
            .held
            .slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&A::ANALYSIS)
            .filter(|slot| slot.inputs == inputs)
            .and_then(|slot| slot.derived.get())
            .map(|derived| Arc::clone(&derived.value))?;
        value.downcast::<A::Value>().ok()
    }

    /// `A`, as held where what it reads still stands, and `derive`'s
    /// otherwise, which is then what is held (#830): a run handed what a page
    /// derived, and a page what a run did.
    async fn held_or<A: Analysed>(
        &self,
        derive: impl Future<Output = A::Value> + Send,
    ) -> Arc<A::Value> {
        let node = Node::Analysis(A::ANALYSIS);
        if let Some(value) = self.standing::<A>(self.inputs(&node).await) {
            return value;
        }
        // What it reads is brought up to date, so an input derived again to
        // what it was leaves this standing.
        let inputs = match self.fetch::<A::Reads>().await {
            Ok(_) => self.inputs(&node).await,
            Err(_) => None,
        };
        let Some(inputs) = inputs else {
            return Arc::new(
                derive
                    .instrument(crate::telemetry::deriving(A::ANALYSIS))
                    .await,
            );
        };
        let (slot, before) = self.held.slot(A::ANALYSIS, inputs);
        if let Some(held) = slot
            .get()
            .and_then(|held| Arc::clone(&held.value).downcast::<A::Value>().ok())
        {
            return held;
        }
        let value = Arc::new(
            derive
                .instrument(crate::telemetry::deriving(A::ANALYSIS))
                .await,
        );
        let _first = slot.set(derived(Arc::clone(&value), inputs, before.as_ref()));
        value
    }

    /// `A`, as held where what it reads still stands, and derived otherwise.
    async fn held<A: Analysed>(&self) -> Result<Arc<A::Value>, NextFailure> {
        let node = Node::Analysis(A::ANALYSIS);
        if let Some(value) = self.standing::<A>(self.inputs(&node).await) {
            return Ok(value);
        }
        // Fetching what it reads derives any input that moved, and an input
        // derived again to what it was keeps its version: then this one is
        // still standing, and is not derived again.
        let read = <A::Reads as Fetch>::fetch(self).await?;
        let inputs = self.inputs(&node).await;
        if let Some(value) = self.standing::<A>(inputs) {
            return Ok(value);
        }
        let derive = async {
            let ports = A::open(self.store, self.zone).await?;
            A::derive(ports, read).await
        }
        .instrument(crate::telemetry::deriving(A::ANALYSIS));
        let Some(inputs) = inputs else {
            return Ok(Arc::new(derive.await?));
        };
        let (slot, before) = self.held.slot(A::ANALYSIS, inputs);
        let held = slot
            .get_or_try_init(|| async {
                Ok::<_, NextFailure>(derived(Arc::new(derive.await?), inputs, before.as_ref()))
            })
            .await?;
        Arc::clone(&held.value).downcast::<A::Value>().map_err(|_| {
            NextFailure::Store(StoreError::Corrupt {
                detail: format!("{} is held as another type", A::ANALYSIS.as_str()),
            })
        })
    }
}

/// Where the plan stands and what is next, as this server holds them, for a
/// run to read and deliver (#830).
///
/// **A load of its own for each read**: a run writes the store between its
/// reads, and a version asked before it gathered would hand back what was
/// held before the gathering moved it.
pub struct HeldNext<'a> {
    held: &'a Held,
    store: &'a Store,
    zone: &'a OperatorZone,
    build: Option<&'a str>,
}

impl<'a> HeldNext<'a> {
    #[must_use]
    pub const fn new(
        held: &'a Held,
        store: &'a Store,
        zone: &'a OperatorZone,
        build: Option<&'a str>,
    ) -> Self {
        Self {
            held,
            store,
            zone,
            build,
        }
    }

    /// A load of what is held, as the store now stands.
    fn holding(&self) -> Holding<'a> {
        Holding::new(self.held, self.store, self.zone, self.build)
    }
}

impl application::next::HeldNodes for HeldNext<'_> {
    /// **Walked as `walk` walks it where it cannot be held**, so a failure
    /// is the walk's to say.
    async fn walked<F>(
        &self,
        now: DayPart,
        walk: F,
    ) -> Result<application::next::Walked, StoreError>
    where
        F: Future<Output = Result<application::next::Walked, StoreError>> + Send,
    {
        let holding = Holding {
            now,
            ..self.holding()
        };
        match Box::pin(holding.held::<crate::analyses::Plans>()).await {
            Ok(walked) => Ok(application::next::Walked {
                standing: walked.standing.clone(),
                diary: walked.diary.clone(),
                plans: walked.plans.now().to_vec(),
                holidays: walked.plans.holidays().cloned(),
            }),
            Err(_) => walk.await,
        }
    }

    async fn upcoming<F>(&self, derive: F) -> Arc<application::next::Upcoming>
    where
        F: Future<Output = application::next::Upcoming> + Send,
    {
        self.holding()
            .held_or::<crate::analyses::Prescription>(derive)
            .await
    }
}

impl crate::next::Holds for HeldNext<'_> {
    fn store(&self) -> &Store {
        self.store
    }

    fn zone(&self) -> &OperatorZone {
        self.zone
    }
}
