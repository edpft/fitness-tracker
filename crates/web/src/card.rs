//! A card, the unit a page is drawn in, and a page as the cards it shows
//! (#830).
//!
//! The operator, 2026-11-16: *"the page is just a collection of cards, the
//! cards are nodes."* **A card is drawn again only when something it reads
//! has moved**, and kept as it was otherwise: in memory, and in the store so a
//! restart still opens at once (#605). **A page has no state of its own**: it
//! is its cards, laid out among words that read nothing.
//!
//! **What a card reads is a type** ([`Fetch`]), and its drawing is handed the
//! values and nothing else, so it cannot read what it does not declare.

use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

use application::{drawing::DrawnFrom, graph::Reads, next::Scope};
use axum::{
    extract::{Query, RawPathParams, State},
    http::HeaderMap,
    response::{IntoResponse as _, Redirect, Response},
};
use configurator::{
    held::{Fetch, Holding},
    telemetry::Served,
};
use domain::analytical::range::Over;
use maud::{Markup, PreEscaped, html};
use std::sync::Arc;
use tracing::Instrument as _;

use crate::{
    range,
    run::{Asked, Runs, Waiting},
};

/// A card: what it reads, and how it is drawn from that.
pub trait Card: Send + Sync + Sized + 'static {
    /// Its name, unique among cards: where it is asked for alone.
    const NAME: &'static str;

    /// What it reads.
    type Reads: Fetch;

    /// The card a request asks for: its range, where it has one.
    fn asked(query: &HashMap<String, String>) -> Self;

    /// The range it is read over, where it offers one.
    fn over(&self) -> Option<Over> {
        None
    }

    /// Draw it from what it read.
    fn render(&self, read: <Self::Reads as Fetch>::Fetched) -> Markup;

    /// Where it is asked for alone, and where its drawing is kept.
    fn address(&self) -> String {
        self.over().map_or_else(
            || format!("{CARDS}{}", Self::NAME),
            |over| format!("{CARDS}{}?over={}", Self::NAME, range::name(over)),
        )
    }
}

/// Where a card is asked for alone.
pub const CARDS: &str = "/card/";

/// A card's drawing, as it was last drawn, and what it was drawn from.
#[derive(Clone)]
pub struct Kept {
    from: Option<DrawnFrom>,
    drawing: String,
}

/// Every card's last drawing, by its address, for as long as the server runs.
#[derive(Default)]
pub struct Drawings(Mutex<HashMap<String, Kept>>);

impl Drawings {
    fn get(&self, address: &str) -> Option<Kept> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(address)
            .cloned()
    }

    fn put(&self, address: String, kept: Kept) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(address, kept);
    }
}

/// One load of a page or a card: what is held, as this load reads it.
pub struct Showing<'a> {
    runs: &'a Runs,
    holding: Holding<'a>,
    redraw: bool,
}

impl<'a> Showing<'a> {
    pub fn new(runs: &'a Runs, redraw: bool) -> Self {
        let setting = runs.setting();
        Self {
            runs,
            holding: Holding::new(runs.held(), &setting.store, &setting.zone, setting.build),
            redraw,
        }
    }

    /// `card`, as it now stands.
    ///
    /// - **While a run collects what it reads**, as it was last drawn, or a
    ///   placeholder where it never has been, asking to be drawn again (#467):
    ///   as each part it waits on is collected where this server's run is
    ///   collecting it, so it is drawn while the run collects the rest
    ///   (#830), and every two seconds where the terminal's is.
    /// - **While nothing it reads has moved**, as it was last drawn, with no
    ///   read (#805).
    /// - **Once something has**, as it was last drawn, asking at once to be
    ///   drawn again, so the page opens without waiting on it.
    /// - **Otherwise drawn**, from what is held and what is derived again.
    pub async fn card<C: Card>(&self, card: &C) -> Markup {
        let span = configurator::telemetry::card(C::NAME);
        self.shown(card).instrument(span).await
    }

    async fn shown<C: Card>(&self, card: &C) -> Markup {
        let address = card.address();
        let reads = <C::Reads as application::graph::Input>::reads();
        let trigger = match self.runs.waiting(&scope(&reads)) {
            Waiting::No => None,
            Waiting::Here(parts) => Some(
                parts
                    .iter()
                    .map(|part| format!("sse:{}", crate::run::event(part)))
                    .chain(["sse:ended".to_owned()])
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            Waiting::Elsewhere => Some("every 2s".to_owned()),
        };
        if let Some(trigger) = trigger {
            let kept = self.kept(&address).await;
            configurator::telemetry::served(if kept.is_some() {
                Served::Kept
            } else {
                Served::Placeholder
            });
            let drawing = kept.map_or_else(
                || html! { section.card { p.collecting { "Collecting what this card shows…" } } }.0,
                |kept| kept.drawing,
            );
            return asking(&address, &drawing, &trigger);
        }
        let from = self.holding.drawn_from(&reads).await;
        let kept = self.kept(&address).await;
        if let Some(kept) = &kept {
            if from.is_some() && kept.from == from {
                configurator::telemetry::served(Served::Unchanged);
                return held(&address, &kept.drawing);
            }
            if !self.redraw {
                configurator::telemetry::served(Served::Moved);
                return asking(&address, &kept.drawing, "load");
            }
        }
        let drawing = match self.holding.fetch::<C::Reads>().await {
            Ok(read) => configurator::telemetry::drawing_card(C::NAME, || card.render(read)),
            Err(error) => {
                return held(
                    &address,
                    &html! { section.card { p.failed { "the store could not be read: " (error) } } }
                        .0,
                );
            }
        };
        configurator::telemetry::served(Served::Drawn);
        if let Some(kept) = &kept {
            configurator::telemetry::same(kept.drawing == drawing.0);
        }
        self.keep(address.clone(), from, &drawing.0);
        held(&address, &drawing.0)
    }

    /// The card at `address` as it was last drawn: by this server, or kept in
    /// the store by one before it.
    async fn kept(&self, address: &str) -> Option<Kept> {
        if let Some(kept) = self.runs.drawings().get(address) {
            return Some(kept);
        }
        let stored = self
            .runs
            .setting()
            .store
            .kept_drawing(address)
            .await
            .ok()
            .flatten()?;
        let kept = Kept {
            from: stored.drawn_from,
            drawing: stored.drawing,
        };
        self.runs.drawings().put(address.to_owned(), kept.clone());
        Some(kept)
    }

    /// Keep `drawing` as the card at `address`: in memory at once, and in
    /// the store without the page waiting on it.
    ///
    /// **A drawing that cannot be stored is let go**: memory still has it,
    /// and a run writing to the store must not hold up the page.
    fn keep(&self, address: String, from: Option<DrawnFrom>, drawing: &str) {
        self.runs.drawings().put(
            address.clone(),
            Kept {
                from,
                drawing: drawing.to_owned(),
            },
        );
        let store = self.runs.setting().store.clone();
        let drawing = drawing.to_owned();
        tokio::spawn(
            async move {
                let _kept = store.keep_drawing(&address, from, &drawing).await;
            }
            .instrument(configurator::telemetry::keeping()),
        );
    }
}

/// The parts a run collecting what `reads` reads takes.
pub fn scope(reads: &Reads) -> Scope {
    configurator::graph::scope(reads)
}

/// A card, in the place it is redrawn into.
fn held(address: &str, drawing: &str) -> Markup {
    html! { div.held data-card=(address) { (PreEscaped(drawing)) } }
}

/// A card as it was, asking to be drawn again, alone, on `trigger`.
fn asking(address: &str, drawing: &str, trigger: &str) -> Markup {
    html! {
        div.held data-card=(address) hx-get=(address) hx-trigger=(trigger) hx-swap="outerHTML" {
            (PreEscaped(drawing))
        }
    }
}

/// The cards a page shows, as a tuple of their types, in the order it lays
/// them out.
pub trait Cards {
    /// Each card, drawn, in order.
    type Shown;

    /// What every card reads.
    fn reads() -> Reads;

    /// Each card asked for alone, routed.
    fn routed(pages: crate::drawn::Pages) -> crate::drawn::Pages;

    /// Each card as the request asks for it, drawn.
    fn shown<'a>(
        query: &'a HashMap<String, String>,
        showing: &'a Showing<'a>,
    ) -> impl Future<Output = Self::Shown> + Send + 'a;
}

macro_rules! tuple_cards {
    ($($name:ident),+) => {
        impl<$($name: Card),+> Cards for ($($name,)+) {
            type Shown = [Markup; tuple_cards!(@count $($name)+)];

            fn reads() -> Reads {
                Reads::default()$(.and(<$name::Reads as application::graph::Input>::reads()))+
            }

            fn routed(pages: crate::drawn::Pages) -> crate::drawn::Pages {
                pages$(.card::<$name>())+
            }

            async fn shown<'a>(
                query: &'a HashMap<String, String>,
                showing: &'a Showing<'a>,
            ) -> Self::Shown {
                [$(showing.card(&$name::asked(query)).await,)+]
            }
        }
    };
    (@count $($name:ident)+) => { 0 $(+ tuple_cards!(@one $name))+ };
    (@one $name:ident) => { 1 };
}

tuple_cards!(A);
tuple_cards!(A, B);
tuple_cards!(A, B, C);
tuple_cards!(A, B, C, D);
tuple_cards!(A, B, C, D, E);
tuple_cards!(A, B, C, D, E, F);
tuple_cards!(A, B, C, D, E, F, G);
tuple_cards!(A, B, C, D, E, F, G, H);
tuple_cards!(A, B, C, D, E, F, G, H, I);
tuple_cards!(A, B, C, D, E, F, G, H, I, J);

/// A page: its cards, laid out among words that read nothing.
pub trait Page: Send + Sync + Sized + 'static {
    /// Where it is routed.
    const ROUTE: &'static str;

    /// The cards it shows.
    type Cards: Cards;

    /// The page asked for, or where to send a request naming none.
    ///
    /// # Errors
    ///
    /// Where to send a request that names no page.
    fn asked(path: &RawPathParams, query: &HashMap<String, String>) -> Result<Self, Redirect>;

    /// The range it is read over, where it offers a choice of one.
    fn over(&self) -> Option<Over> {
        None
    }

    /// The page, its cards laid out.
    fn layout(&self, cards: <Self::Cards as Cards>::Shown) -> Markup;
}

/// What a page's cards read: what a run opening it collects.
pub fn reads<P: Page>() -> Reads {
    <P::Cards as Cards>::reads()
}

/// `P` as the request asks for it: each card as it now stands.
///
/// **Opening it starts a run** of what its cards read, unless htmx asks,
/// which is a page redrawing itself: starting another would redraw it again
/// when that one ended, for ever.
///
/// **Each card follows the run itself** (#830), so the page is not drawn
/// again when the run ends.
pub async fn page<P: Page>(
    State(runs): State<Arc<Runs>>,
    headers: HeaderMap,
    path: RawPathParams,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let page = match P::asked(&path, &query) {
        Ok(page) => page,
        Err(elsewhere) => return elsewhere.into_response(),
    };
    if let Some(over) = page.over() {
        configurator::telemetry::ranged(range::name(over));
    }
    let redraw = headers.contains_key("hx-request");
    if !redraw {
        runs.open(&Asked::Only(scope(&reads::<P>()))).await;
    }
    let view = runs.view();
    let showing = Showing::new(&runs, redraw);
    let shown = <P::Cards as Cards>::shown(&query, &showing).await;
    crate::page::carded(&page.layout(shown).0, &view).into_response()
}

/// `C` alone, as the request asks for it: what a card asking to be drawn
/// again is sent.
pub async fn card<C: Card>(
    State(runs): State<Arc<Runs>>,
    Query(query): Query<HashMap<String, String>>,
) -> Markup {
    let card = C::asked(&query);
    if let Some(over) = card.over() {
        configurator::telemetry::ranged(range::name(over));
    }
    Showing::new(&runs, true).card(&card).await
}

/// A card over a range that reads one analysis, the plans its ranges are
/// drawn from, and today: `$draw` draws it from the analysis's value, the
/// ranges and the range chosen.
///
/// `over = $over` names how a request with no range is read, where it is not
/// the macrocycle.
macro_rules! ranged_card {
    (
        $(#[$meta:meta])*
        $card:ident, $name:literal, $analysis:ty, $draw:path $(, over = $over:path)? $(,)?
    ) => {
        $(#[$meta])*
        pub struct $card {
            over: domain::analytical::range::Over,
        }

        impl crate::card::Card for $card {
            const NAME: &'static str = $name;
            type Reads = (
                configurator::held::Of<$analysis>,
                configurator::held::Of<configurator::analyses::Plans>,
                configurator::clock::Today,
            );

            fn asked(query: &std::collections::HashMap<String, String>) -> Self {
                Self {
                    over: crate::card::ranged_card!(@over query $($over)?),
                }
            }

            fn over(&self) -> Option<domain::analytical::range::Over> {
                Some(self.over)
            }

            fn render(
                &self,
                (read, plans, today): (
                    std::sync::Arc<<$analysis as configurator::held::Analysed>::Value>,
                    std::sync::Arc<configurator::rescheduling::Plans>,
                    jiff::civil::Date,
                ),
            ) -> maud::Markup {
                $draw(
                    &read,
                    &application::ranges::of(today, plans.now()),
                    self.over,
                )
            }
        }
    };
    (@over $query:ident) => { crate::drawn::over($query) };
    (@over $query:ident $over:path) => { $over($query) };
}
pub(crate) use ranged_card;

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use application::graph::{Input, Node, Reads};
    use configurator::{
        clock::Today,
        held::{Fetch, FetchFuture, Holding},
    };
    use domain::normalised::OperatorZone;
    use maud::{Markup, html};

    use super::{Card, Showing};
    use crate::run::{Runs, Setting};

    /// The diary, read for its version and nothing else.
    struct Diary;

    impl Input for Diary {
        fn reads() -> Reads {
            Reads::of([Node::DIARY])
        }
    }

    impl Fetch for Diary {
        type Fetched = ();

        fn fetch<'a>(_: &'a Holding<'a>) -> FetchFuture<'a, ()> {
            Box::pin(std::future::ready(Ok(())))
        }
    }

    static DIARIED: AtomicUsize = AtomicUsize::new(0);
    static DATED: AtomicUsize = AtomicUsize::new(0);

    /// A card reading the diary and the date, counting its drawings.
    struct Diaried;

    impl Card for Diaried {
        const NAME: &'static str = "test-diaried";
        type Reads = (Diary, Today);

        fn asked(_: &HashMap<String, String>) -> Self {
            Self
        }

        fn render(&self, _: ((), jiff::civil::Date)) -> Markup {
            let drawn = DIARIED.fetch_add(1, Ordering::SeqCst) + 1;
            html! { section.card { p { "diaried " (drawn) } } }
        }
    }

    /// A card reading the date alone, counting its drawings.
    struct Dated;

    impl Card for Dated {
        const NAME: &'static str = "test-dated";
        type Reads = Today;

        fn asked(_: &HashMap<String, String>) -> Self {
            Self
        }

        fn render(&self, _: jiff::civil::Date) -> Markup {
            let drawn = DATED.fetch_add(1, Ordering::SeqCst) + 1;
            html! { section.card { p { "dated " (drawn) } } }
        }
    }

    /// The weigh-ins, normalised, read for their version and nothing else.
    struct Weighed;

    impl Input for Weighed {
        fn reads() -> Reads {
            Reads::of([Node::Normalised(
                domain::landing::LandingStream::try_from("withings.measurements")
                    .expect("a stream"),
            )])
        }
    }

    impl Fetch for Weighed {
        type Fetched = ();

        fn fetch<'a>(_: &'a Holding<'a>) -> FetchFuture<'a, ()> {
            Box::pin(std::future::ready(Ok(())))
        }
    }

    static WEIGHT: AtomicUsize = AtomicUsize::new(0);

    /// A card reading the weigh-ins, counting its drawings.
    struct Weight;

    impl Card for Weight {
        const NAME: &'static str = "test-weight";
        type Reads = Weighed;

        fn asked(_: &HashMap<String, String>) -> Self {
            Self
        }

        fn render(&self, (): ()) -> Markup {
            let drawn = WEIGHT.fetch_add(1, Ordering::SeqCst) + 1;
            html! { section.card { p { "weight " (drawn) } } }
        }
    }

    /// **A card is drawn as soon as what it reads is collected** (#830),
    /// while the run collects the rest: until then it asks to be drawn again
    /// on each part it still waits on, and on the run ending; the card beside
    /// it, reading nothing the run collects, is drawn at once.
    #[test]
    fn a_card_is_drawn_once_what_it_reads_is_collected_mid_run() {
        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let runs = Runs::new(Setting {
            store: runtime
                .block_on(configurator::store::Store::open(
                    &directory.path().join("fitness.db"),
                ))
                .expect("the store"),
            zone: OperatorZone::try_from("Europe/London").expect("a zone"),
            credentials: None,
            build: Some("a build"),
        });
        let load = || -> (String, String) {
            runtime.block_on(async {
                let showing = Showing::new(&runs, true);
                (
                    showing.card(&Weight).await.into_string(),
                    showing.card(&Dated).await.into_string(),
                )
            })
        };
        let stream =
            domain::landing::LandingStream::try_from("withings.measurements").expect("a stream");
        runs.going_over(crate::run::Asked::Everything);

        let (weight, dated) = load();
        assert!(
            weight.contains("Collecting what this card shows"),
            "{weight}"
        );
        assert!(
            weight.contains(
                r#"hx-trigger="sse:fetched-withings.measurements, sse:normalised-withings.measurements, sse:ended""#
            ),
            "{weight}"
        );
        assert!(!dated.contains("hx-trigger"), "{dated}");

        runs.collect(&[application::Part::Fetching(stream.clone())]);
        let (weight, _) = load();
        assert!(
            weight.contains(r#"hx-trigger="sse:normalised-withings.measurements, sse:ended""#),
            "{weight}"
        );
        assert_eq!(WEIGHT.load(Ordering::SeqCst), 0, "not drawn while it waits");

        runs.collect(&[application::Part::Normalising(stream)]);
        let (weight, _) = load();
        assert!(weight.contains("weight 1"), "{weight}");
        assert!(!weight.contains("hx-trigger"), "{weight}");
    }

    /// **A card is drawn again only when something it reads has moved**
    /// (#830): after the diary is written, the card reading it is served as
    /// it was, asking to be drawn again, and drawn when it asks; the card
    /// beside it, which reads only the date, is served as it was with no
    /// drawing at all.
    #[test]
    fn a_write_redraws_only_the_card_reading_what_it_wrote() {
        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let runs = Runs::new(Setting {
            store: runtime
                .block_on(configurator::store::Store::open(
                    &directory.path().join("fitness.db"),
                ))
                .expect("the store"),
            zone: OperatorZone::try_from("Europe/London").expect("a zone"),
            credentials: None,
            build: Some("a build"),
        });
        let load = |redraw: bool| -> (String, String) {
            runtime.block_on(async {
                let showing = Showing::new(&runs, redraw);
                (
                    showing.card(&Diaried).await.into_string(),
                    showing.card(&Dated).await.into_string(),
                )
            })
        };

        let (diaried, dated) = load(false);
        assert!(diaried.contains("diaried 1"), "{diaried}");
        assert!(dated.contains("dated 1"), "{dated}");

        let (diaried, dated) = load(false);
        assert!(diaried.contains("diaried 1"), "{diaried}");
        assert!(!diaried.contains("hx-trigger"), "{diaried}");
        assert!(dated.contains("dated 1"), "{dated}");
        assert_eq!(DIARIED.load(Ordering::SeqCst), 1, "nothing it reads moved");

        runtime
            .block_on(configurator::scheduling::ill(
                &runs.setting().store,
                jiff::civil::date(2026, 11, 16),
            ))
            .expect("the diary written");
        let (diaried, dated) = load(false);
        assert!(diaried.contains("diaried 1"), "{diaried}");
        assert!(diaried.contains(r#"hx-trigger="load""#), "{diaried}");
        assert!(dated.contains("dated 1"), "{dated}");
        assert!(!dated.contains("hx-trigger"), "{dated}");

        let (diaried, dated) = load(true);
        assert!(diaried.contains("diaried 2"), "{diaried}");
        assert!(dated.contains("dated 1"), "{dated}");
        assert_eq!(DATED.load(Ordering::SeqCst), 1, "the date never moved");
    }
}
