//! A page served from its kept drawing while nothing it reads has moved
//! (#805), and the router that takes only such pages.
//!
//! **Keyed on what it was drawn from**, which is read from the ledger and the
//! revisions and never from the layers: a page whose key is unchanged is the
//! drawing kept, served without a read. One whose key has moved is served the
//! drawing kept at once, and asks to be drawn again in place.

use std::{collections::HashMap, sync::Arc};

use application::{graph::Reads, next::Scope};
use axum::{
    Router,
    extract::{Query, RawPathParams, State},
    http::HeaderMap,
    response::{IntoResponse as _, Redirect, Response},
    routing::{MethodRouter, get},
};
use configurator::{clock::Now, store::Store, telemetry::Served};
use domain::{analytical::range::Over, normalised::OperatorZone};
use maud::Markup;
use tracing::Instrument as _;

use crate::{
    page, range,
    run::{self, Asked, Runs},
};

/// A page drawn from the store, and served as it was last drawn while nothing
/// it reads has moved.
pub trait Drawn: Sized + Send + Sync + 'static {
    /// Where it is routed.
    const ROUTE: &'static str;

    /// **Whether opening it starts a run** of what it reads: not for a page
    /// opened from one that did, which waits on that run.
    const OPENS: bool = true;

    /// What it is drawn from.
    type Read: Send;

    /// What it reads: so what a run opening it collects, and what its kept
    /// drawing is drawn from.
    fn reads() -> Reads;

    /// The page asked for, or where to send a request naming none.
    ///
    /// # Errors
    ///
    /// Where to send a request that names no page.
    fn asked(path: &RawPathParams, query: &HashMap<String, String>) -> Result<Self, Redirect>;

    /// Its path.
    fn here(&self) -> String;

    /// The range it is read over, where it offers a choice of one.
    fn over(&self) -> Option<Over> {
        None
    }

    /// Read what it is drawn from.
    fn read(
        &self,
        store: &Store,
        zone: &OperatorZone,
    ) -> impl Future<Output = Result<Self::Read, Unreadable>> + Send;

    /// Draw it from what was read, or say why that could not be.
    fn render(&self, read: Result<Self::Read, Unreadable>) -> Markup;

    /// Where its drawing is kept: its path, and its range where it has one.
    fn address(&self) -> String {
        self.over().map_or_else(
            || self.here(),
            |over| format!("{}?over={}", self.here(), range::name(over)),
        )
    }
}

/// Why a page could not be read, as it says so.
pub struct Unreadable(pub String);

impl<E: std::fmt::Display> From<E> for Unreadable {
    fn from(error: E) -> Self {
        Self(error.to_string())
    }
}

/// The router, taking a page only as [`Drawn`], so a page cannot be served
/// that is not served its kept drawing (#805).
///
/// **What is not a drawn page says what it is instead**: a form, which shows
/// what was just written, the run as it goes, or a file the pages load.
pub struct Pages {
    router: Router<Arc<Runs>>,
    reads: Vec<(&'static str, Reads)>,
    cards: std::collections::BTreeMap<&'static str, Reads>,
}

impl Pages {
    pub fn new() -> Self {
        Self {
            router: Router::new(),
            reads: Vec::new(),
            cards: std::collections::BTreeMap::new(),
        }
    }

    /// `P`, drawn as its cards (#830), and each card asked for alone.
    pub fn cards<P: crate::card::Page>(mut self) -> Self {
        self.router = self.router.route(P::ROUTE, get(crate::card::page::<P>));
        self.reads.push((P::ROUTE, crate::card::reads::<P>()));
        <P::Cards as crate::card::Cards>::routed(self)
    }

    /// `C`, asked for alone: routed once, however many pages show it.
    pub fn card<C: crate::card::Card>(mut self) -> Self {
        if self
            .cards
            .insert(C::NAME, <C::Reads as application::graph::Input>::reads())
            .is_none()
        {
            self.router = self.router.route(
                &format!("{}{}", crate::card::CARDS, C::NAME),
                get(crate::card::card::<C>),
            );
        }
        self
    }

    /// `P`, served its kept drawing while nothing it reads has moved.
    pub fn page<P: Drawn>(mut self) -> Self {
        self.router = self.router.route(P::ROUTE, get(served::<P>));
        self.reads.push((P::ROUTE, P::reads()));
        self
    }

    /// A page that writes what is posted to it and shows it again, drawn every
    /// time it is opened: what it shows is what was just written.
    pub fn form(mut self, path: &str, form: MethodRouter<Arc<Runs>>) -> Self {
        self.router = self.router.route(path, form);
        self
    }

    /// The run as it goes, which follows the run rather than the store.
    pub fn live(mut self, path: &str, live: MethodRouter<Arc<Runs>>) -> Self {
        self.router = self.router.route(path, live);
        self
    }

    /// A file every page loads, or an address a page has moved from.
    pub fn file(mut self, path: &str, file: MethodRouter<Arc<Runs>>) -> Self {
        self.router = self.router.route(path, file);
        self
    }

    /// Each drawn page's route and what it reads.
    #[cfg(test)]
    pub fn reads(&self) -> &[(&'static str, Reads)] {
        &self.reads
    }

    /// Each card routed, by its name, and what it reads.
    #[cfg(test)]
    pub const fn cards_read(&self) -> &std::collections::BTreeMap<&'static str, Reads> {
        &self.cards
    }

    pub fn with_state(self, runs: Arc<Runs>) -> Router {
        self.router.with_state(runs)
    }
}

/// `P` as the request asks for it.
async fn served<P: Drawn>(
    State(runs): State<Arc<Runs>>,
    headers: HeaderMap,
    path: RawPathParams,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let page = match P::asked(&path, &query) {
        Ok(page) => page,
        Err(elsewhere) => return elsewhere.into_response(),
    };
    let scope = configurator::graph::scope(&P::reads());
    // **Not when htmx asks**, which is a page redrawing itself because a run
    // ended: starting another would redraw it again when that one ended, for
    // ever.
    let redraw = headers.contains_key("hx-request");
    if P::OPENS && !redraw {
        runs.open(&Asked::Only(scope.clone())).await;
    }
    let run = runs.view();
    drawn(&runs, &page, &scope, &run, redraw)
        .await
        .into_response()
}

/// `page`, as it was last drawn where nothing it reads has moved since, and
/// drawn from the store otherwise; while a run is collecting what it reads,
/// as it was last drawn, and the placeholder where it has not been (#467).
///
/// **Asked before the store is read and again after**, so a run that starts
/// while the page is being drawn is not missed: the read is one snapshot, but
/// one taken after a layer was emptied and before it was rebuilt.
///
/// **Keyed before it is read**, so a write landing while it is drawn is one
/// its key does not include, and the next load draws it again.
///
/// **The last drawing is kept in the store** (#605), so a page opened just
/// after `fitness-web` restarts is shown at once rather than the placeholder.
pub async fn drawn<P: Drawn>(
    runs: &Runs,
    page: &P,
    scope: &Scope,
    run: &run::View,
    redraw: bool,
) -> Markup {
    if let Some(over) = page.over() {
        configurator::telemetry::ranged(range::name(over));
    }
    let address = page.address();
    if runs.collecting(scope) {
        return Box::pin(meanwhile(runs, page, &address, run)).await;
    }
    let setting = runs.setting();
    let store = &setting.store;
    let drawn_from = store
        .drawn_from(&P::reads(), setting.build, &Now::new(setting.zone.clone()))
        .await
        .ok()
        .flatten();
    let kept = store.kept_drawing(&address).await.ok().flatten();
    if let Some(kept) = &kept {
        if drawn_from.is_some() && kept.drawn_from == drawn_from {
            configurator::telemetry::served(Served::Unchanged);
            return page::followed(&kept.drawing, run);
        }
        if !redraw {
            configurator::telemetry::served(Served::Moved);
            return page::followed(&page::moved(&kept.drawing).0, run);
        }
    }
    let read = page.read(store, &setting.zone).await;
    let drawing = configurator::telemetry::rendering(|| page.render(read));
    if runs.collecting(scope) {
        return Box::pin(meanwhile(runs, page, &address, run)).await;
    }
    configurator::telemetry::served(Served::Drawn);
    if let Some(kept) = &kept {
        configurator::telemetry::same(kept.drawing == drawing.0);
    }
    keep(store, address, drawn_from, &drawing);
    page::followed(&drawing.0, run)
}

/// `page` as it was last drawn, asking to be drawn again, or the placeholder
/// where it has not been.
///
/// **A drawing that cannot be read is one not kept**: the placeholder costs a
/// wait, and saying the store is unreadable would be wrong about the page.
async fn meanwhile<P: Drawn>(runs: &Runs, page: &P, address: &str, run: &run::View) -> Markup {
    let kept = runs
        .setting()
        .store
        .kept_drawing(address)
        .await
        .ok()
        .flatten();
    kept.map_or_else(
        || {
            configurator::telemetry::served(Served::Placeholder);
            page::collecting(&page.here(), run)
        },
        |kept| {
            configurator::telemetry::served(Served::Kept);
            page::followed(&page::kept(&kept.drawing).0, run)
        },
    )
}

/// Keep `drawn` as the page at `address`, without the page waiting on it.
///
/// **A drawing that cannot be kept is let go**: it is drawn again the next
/// time the page is opened with nothing collecting, and a run writing to the
/// store must not hold up the page that is already drawn.
fn keep(
    store: &Store,
    address: String,
    drawn_from: Option<application::drawing::DrawnFrom>,
    drawn: &Markup,
) {
    let store = store.clone();
    let drawing = drawn.0.clone();
    tokio::spawn(
        async move {
            let _kept = store.keep_drawing(&address, drawn_from, &drawing).await;
        }
        .instrument(configurator::telemetry::keeping()),
    );
}

/// The range `?over=` names.
pub fn over(query: &HashMap<String, String>) -> Over {
    range::named(query.get("over").map(String::as_str))
}

/// The range `?over=` names, or all where it names none: the FTP tests open
/// on All, since there are few in a macrocycle (operator, 2026-10-28).
pub fn tests_over(query: &HashMap<String, String>) -> Over {
    query
        .get("over")
        .map_or(Over::All, |over| range::named(Some(over)))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use application::{
        Part,
        graph::{Node, Reads},
        next::Scope,
    };
    use axum::{extract::RawPathParams, response::Redirect};
    use configurator::store::Store;
    use domain::{analytical::range::Over, landing::LandingStream, normalised::OperatorZone};
    use maud::{Markup, html};

    use super::{Drawn, Unreadable, drawn};
    use crate::run::{Asked, Runs, Setting, View};

    /// A page reading the diary, drawn as `shown`, counting its reads.
    struct Shown {
        over: Option<Over>,
        text: &'static str,
        read: Arc<AtomicUsize>,
    }

    impl Drawn for Shown {
        const ROUTE: &'static str = "/estimated-1rm";
        type Read = ();

        fn reads() -> Reads {
            Reads::of([Node::DIARY, Node::Date])
        }

        fn asked(_: &RawPathParams, _: &HashMap<String, String>) -> Result<Self, Redirect> {
            Ok(Self {
                over: None,
                text: "",
                read: Arc::default(),
            })
        }

        fn here(&self) -> String {
            Self::ROUTE.to_owned()
        }

        fn over(&self) -> Option<Over> {
            self.over
        }

        async fn read(&self, _: &Store, _: &OperatorZone) -> Result<(), Unreadable> {
            self.read.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn render(&self, _: Result<(), Unreadable>) -> Markup {
            html! { html { body { main { p { (self.text) } } } } }
        }
    }

    struct Serving {
        runtime: tokio::runtime::Runtime,
        database: std::path::PathBuf,
        _directory: tempfile::TempDir,
    }

    impl Serving {
        fn new() -> Self {
            let directory = tempfile::TempDir::new().expect("a temporary directory");
            Self {
                runtime: tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("a runtime"),
                database: directory.path().join("fitness.db"),
                _directory: directory,
            }
        }

        /// A server started afresh, which opens the store afresh as `serve`
        /// does, serving `build`.
        fn fresh(&self, build: Option<&'static str>) -> Runs {
            Runs::new(Setting {
                store: self
                    .runtime
                    .block_on(Store::open(&self.database))
                    .expect("the store"),
                zone: OperatorZone::try_from("Europe/London").expect("a zone"),
                credentials: None,
                build,
            })
        }

        fn drawn(&self, runs: &Runs, page: &Shown, redraw: bool) -> String {
            let collects = Scope::new([Part::Normalising(
                LandingStream::try_from("hevy.workouts").expect("a stream"),
            )]);
            self.runtime
                .block_on(drawn(runs, page, &collects, &View::default(), redraw))
                .into_string()
        }

        /// Wait for `address` to be kept as `shown`: it is kept without the
        /// page waiting on it.
        fn kept(&self, runs: &Runs, address: &str, shown: &str) {
            for _ in 0..100 {
                let kept = self
                    .runtime
                    .block_on(runs.setting().store.kept_drawing(address))
                    .expect("a read");
                if kept.is_some_and(|kept| kept.drawing.contains(shown)) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            panic!("{address} was never kept as {shown}");
        }
    }

    fn shown(text: &'static str, read: &Arc<AtomicUsize>) -> Shown {
        Shown {
            over: None,
            text,
            read: Arc::clone(read),
        }
    }

    /// **A page whose data a run is collecting is drawn as it was before, and
    /// the store is not read** (#467): on 2026-10-16 the estimated 1RM page
    /// read the gym's canonical layer while a run had emptied it, and said *"No
    /// test has been recorded."* A page never drawn on this store is the
    /// placeholder.
    ///
    /// **Kept across a restart** (#605): a server started afresh on the same
    /// store, a run going, is served the drawing the last one kept.
    #[test]
    fn a_page_is_kept_as_it_was_while_its_data_is_collected() {
        let serving = Serving::new();
        let read = Arc::default();
        let runs = serving.fresh(None);
        assert!(
            serving
                .drawn(&runs, &shown("Front squat 92.5kg", &read), false)
                .contains("Front squat 92.5kg")
        );
        serving.kept(&runs, "/estimated-1rm", "Front squat 92.5kg");

        runs.going_over(Asked::Everything);
        let page = shown("No test has been recorded.", &read);
        let shown_now = serving.drawn(&runs, &page, false);
        assert!(shown_now.contains("Front squat 92.5kg"), "{shown_now}");
        assert!(!shown_now.contains("No test"), "{shown_now}");
        assert!(
            shown_now.contains(r#"hx-trigger="every 2s""#),
            "{shown_now}"
        );

        let restarted = serving.fresh(None);
        restarted.going_over(Asked::Everything);
        let shown_now = serving.drawn(&restarted, &page, false);
        assert!(shown_now.contains("Front squat 92.5kg"), "{shown_now}");
        assert!(!shown_now.contains("No test"), "{shown_now}");

        let ranged = Shown {
            over: Some(Over::All),
            ..shown("No test has been recorded.", &read)
        };
        let shown_now = serving.drawn(&restarted, &ranged, false);
        assert!(
            shown_now.contains("Collecting what this page shows…"),
            "{shown_now}"
        );
        assert!(
            shown_now.contains("<title>Estimated 1RM</title>"),
            "{shown_now}"
        );
        assert!(!shown_now.contains("No test"), "{shown_now}");
        assert_eq!(read.load(Ordering::SeqCst), 1, "read only when drawn");
    }

    /// **A page is served as it was drawn while nothing it reads has moved,
    /// without a read** (#805), even by a server started afresh. **One whose
    /// diary was written is served as it was at once, asking to be drawn
    /// again**, and drawn again when it asks. **A build drawing it afresh
    /// draws it**, since another build may draw it differently.
    #[test]
    fn a_page_is_served_as_drawn_while_nothing_it_reads_moves() {
        let serving = Serving::new();
        let read = Arc::default();
        let runs = serving.fresh(Some("a build"));
        serving.drawn(&runs, &shown("Planned", &read), false);
        serving.kept(&runs, "/estimated-1rm", "Planned");
        assert_eq!(read.load(Ordering::SeqCst), 1);

        let restarted = serving.fresh(Some("a build"));
        let served = serving.drawn(&restarted, &shown("Drawn again", &read), false);
        assert!(served.contains("Planned"), "{served}");
        assert!(!served.contains(r#"hx-trigger="load""#), "{served}");
        assert_eq!(read.load(Ordering::SeqCst), 1, "served without a read");

        serving
            .runtime
            .block_on(configurator::scheduling::ill(
                &restarted.setting().store,
                jiff::civil::date(2026, 11, 14),
            ))
            .expect("the diary written");
        let served = serving.drawn(&restarted, &shown("Ill", &read), false);
        assert!(served.contains("Planned"), "{served}");
        assert!(served.contains(r#"hx-trigger="load""#), "{served}");
        assert_eq!(read.load(Ordering::SeqCst), 1, "served as it was at once");

        let redrawn = serving.drawn(&restarted, &shown("Ill", &read), true);
        assert!(redrawn.contains("Ill"), "{redrawn}");
        assert_eq!(read.load(Ordering::SeqCst), 2, "drawn again when it asks");
        serving.kept(&restarted, "/estimated-1rm", "Ill");
        let served = serving.drawn(&restarted, &shown("Again", &read), false);
        assert!(served.contains("Ill"), "{served}");
        assert_eq!(read.load(Ordering::SeqCst), 2);

        let rebuilt = serving.fresh(Some("another build"));
        let served = serving.drawn(&rebuilt, &shown("Rebuilt", &read), true);
        assert!(served.contains("Rebuilt"), "{served}");
        assert_eq!(read.load(Ordering::SeqCst), 3);
    }
}
