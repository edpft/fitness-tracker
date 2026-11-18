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
    type Value: Send + Sync + 'static;

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

/// A value held, and the version of what it was derived from.
struct Slot {
    from: DrawnFrom,
    value: Arc<OnceCell<Arc<dyn Any + Send + Sync>>>,
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
    /// The slot for `analysis` derived from `from`, emptied if what it held
    /// was derived from anything else.
    fn slot(
        &self,
        analysis: Analysis,
        from: DrawnFrom,
    ) -> Arc<OnceCell<Arc<dyn Any + Send + Sync>>> {
        let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        match slots.get(&analysis) {
            Some(slot) if slot.from == from => Arc::clone(&slot.value),
            _ => {
                let value = Arc::new(OnceCell::new());
                slots.insert(
                    analysis,
                    Slot {
                        from,
                        value: Arc::clone(&value),
                    },
                );
                value
            }
        }
    }
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
    /// it has no version, or the build is not known.
    ///
    /// **A store that cannot say has no version**, and what reads it is
    /// derived again rather than served as it was.
    pub async fn drawn_from(&self, reads: &Reads) -> Option<DrawnFrom> {
        let build = self.build?;
        self.drawing
            .drawn_from(crate::graph::graph(), reads, build, self.now)
            .instrument(crate::telemetry::keying())
            .await
            .ok()
            .flatten()
    }

    /// Fetch what `F` reads.
    ///
    /// # Errors
    ///
    /// [`NextFailure`] if the store cannot be read.
    pub async fn fetch<F: Fetch>(&self) -> Result<F::Fetched, NextFailure> {
        F::fetch(self).await
    }

    /// `A`, as held where what it reads still stands, and `derive`'s
    /// otherwise, which is then what is held (#830): a run handed what a page
    /// derived, and a page what a run did.
    async fn held_or<A: Analysed>(
        &self,
        derive: impl Future<Output = A::Value> + Send,
    ) -> Arc<A::Value> {
        let node = Node::Analysis(A::ANALYSIS);
        let slot = self
            .drawn_from(&Reads::of([node]))
            .await
            .map(|from| self.held.slot(A::ANALYSIS, from));
        if let Some(value) = slot
            .as_ref()
            .and_then(|slot| slot.get())
            .and_then(|value| Arc::clone(value).downcast::<A::Value>().ok())
        {
            return value;
        }
        let derived = Arc::new(
            derive
                .instrument(crate::telemetry::deriving(A::ANALYSIS))
                .await,
        );
        if let Some(slot) = slot {
            let held: Arc<dyn Any + Send + Sync> = Arc::clone(&derived) as _;
            let _first = slot.set(held);
        }
        derived
    }

    /// `A`, as held where what it reads still stands, and derived otherwise.
    async fn held<A: Analysed>(&self) -> Result<Arc<A::Value>, NextFailure> {
        let node = Node::Analysis(A::ANALYSIS);
        let value = self.drawn_from(&Reads::of([node])).await.map_or_else(
            || Arc::new(OnceCell::new()),
            |from| self.held.slot(A::ANALYSIS, from),
        );
        let value = value
            .get_or_try_init(|| async {
                let read = <A::Reads as Fetch>::fetch(self).await?;
                let derived = async {
                    let ports = A::open(self.store, self.zone).await?;
                    A::derive(ports, read).await
                }
                .instrument(crate::telemetry::deriving(A::ANALYSIS))
                .await?;
                let value: Arc<dyn Any + Send + Sync> = Arc::new(derived);
                Ok::<_, NextFailure>(value)
            })
            .await?;
        Arc::clone(value).downcast::<A::Value>().map_err(|_| {
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
        match holding.held::<crate::analyses::Plans>().await {
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
