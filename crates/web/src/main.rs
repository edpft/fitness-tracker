//! The HTTP driving adapter: `next` from a browser (#398).
//!
//! Opening a page starts a run of `next` (#466), and each page is its cards,
//! each drawn again as what it reads moves (#830): the front page's from what
//! is next and where the plan stands, which the run issues and delivers from.
//! The use case comes from the configurator, so this crate requests, renders
//! and interacts, and names no adapter.

mod at_power;
mod body;
mod card;
mod collected;
mod composition;
mod curves;
mod detail;
mod drawn;
mod equipment;
mod ftp;
mod history;
mod home;
mod hrv;
mod input;
mod one_rep_max;
mod page;
mod plan;
mod progress;
mod progression;
mod pulse_wave_velocity;
mod range;
mod report;
mod resting;
mod run;
mod schedule;
mod sleep;
mod summary;
mod weight;
mod working;
mod workout;

use std::{
    collections::HashMap,
    convert::Infallible,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    Form, Router,
    extract::{Query, RawQuery, Request, State},
    http::header,
    middleware::{self, Next},
    response::{
        IntoResponse, Redirect, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use clap::{Arg, ArgMatches, Command, value_parser};
use maud::Markup;
use tokio_stream::{Stream, StreamExt as _, wrappers::WatchStream};
use tracing::Instrument as _;

use domain::planner::Relative;
use drawn::Pages;
use run::{Runs, Setting};

fn main() -> std::process::ExitCode {
    // `clippy::exit` is `forbid`, so returning `ExitCode` is not a style
    // preference: `std::process::exit` will not compile.
    let matches = match command().try_get_matches() {
        Ok(matches) => matches,
        Err(error) => {
            let _ = error.print();
            return if error.use_stderr() {
                std::process::ExitCode::from(2)
            } else {
                std::process::ExitCode::SUCCESS
            };
        }
    };
    let served = database(&matches).and_then(|database| {
        // **Dropped after the runtime, never before** (#817): a span closes
        // when its last child does, and a child can be in a task the runtime
        // holds, so a recording dropped first loses every span still open.
        let _recording = configurator::telemetry::record(&database);
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| runtime.block_on(serve(&matches, &database)))
    });
    match served {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("web: {message}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn command() -> Command {
    Command::new("web")
        .about("Serve the page that runs next")
        .arg(
            // **No default** (§ 34): which address the page is reachable at is
            // a fact about the machine and the network, and a default would be
            // an assumption about both.
            Arg::new("listen")
                .long("listen")
                .env("FITNESS_WEB_LISTEN")
                .required(true)
                .value_parser(value_parser!(SocketAddr))
                .help("The address and port to serve on, such as 127.0.0.1:8080"),
        )
        .arg(
            Arg::new("database")
                .long("database")
                .env("FITNESS_TRACKER_DATABASE")
                .value_parser(value_parser!(PathBuf))
                .help("Where the store lives. No path is compiled in"),
        )
        .arg(
            Arg::new("timezone")
                .long("timezone")
                .env("FITNESS_TRACKER_TIMEZONE")
                .help("The IANA time zone trained in, such as Europe/London"),
        )
}

/// Resolve what every run reads, then serve until stopped.
/// Where the store is: what this invocation passed, else the default.
fn database(matches: &ArgMatches) -> Result<PathBuf, String> {
    matches.get_one::<PathBuf>("database").map_or_else(
        || {
            configurator::paths::store(&configurator::paths::SystemEnvironment)
                .map_err(|error| error.to_string())
        },
        |path| Ok(path.clone()),
    )
}

async fn serve(matches: &ArgMatches, database: &Path) -> Result<(), String> {
    // **Opened once, here, and shared by every page** (#663): a pool per page
    // load cost each one more than the reads it was opened for.
    let store = configurator::store::Store::open(database)
        .await
        .map_err(|error| error.to_string())?;
    let stated = store.timezone().await.map_err(|error| error.to_string())?;
    let zone = configurator::config::timezone(
        matches.get_one::<String>("timezone").map(String::as_str),
        stated.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    // **Where they are kept, not what was in them** (#438): each run reads
    // them, so `fitness credentials` takes effect without a restart.
    let credentials =
        configurator::paths::credentials(&configurator::paths::SystemEnvironment).ok();

    let runs = Arc::new(Runs::new(Setting {
        store,
        zone,
        credentials,
        build: configurator::next::build(),
    }));
    let app = app(runs);

    let Some(address) = matches.get_one::<SocketAddr>("listen") else {
        return Err("--listen is required".to_owned());
    };
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("cannot listen on {address}: {error}"))?;
    eprintln!("web: serving on http://{address}");
    axum::serve(listener, app)
        .await
        .map_err(|error| error.to_string())
}

/// Every page and what they load.
///
/// **Every route is recorded** (#804), because the layer is on the whole
/// router: a page added later is recorded without being listed anywhere.
fn app(runs: Arc<Runs>) -> Router {
    pages()
        .with_state(runs)
        .layer(middleware::from_fn(recorded))
}

/// Every page, each drawn page as its type (#805).
fn pages() -> Pages {
    Pages::new()
        .cards::<home::Page>()
        .page::<plan::Page>()
        .cards::<at_power::Page>()
        .cards::<ftp::Page>()
        .cards::<body::Page>()
        .cards::<pulse_wave_velocity::Page>()
        .cards::<hrv::Page>()
        .cards::<resting::Page>()
        .cards::<sleep::Page>()
        .cards::<one_rep_max::Page>()
        .cards::<working::Page>()
        .cards::<progress::Page>()
        .page::<history::Page>()
        .page::<history::Opened>()
        .file(body::REPLACED[0], get(to_body))
        .file(body::REPLACED[1], get(to_body))
        .form("/equipment", get(equipment_page).post(equipment_post))
        .form(schedule::HERE, get(schedule_page).post(schedule_post))
        .form(workout::HERE, get(workout_page).post(workout_post))
        .live("/run/events", get(events))
        .live("/run/step", get(step))
        .file("/htmx.min.js", get(htmx))
        .file("/sse.min.js", get(sse))
        .file(page::STYLESHEET, get(stylesheet))
}

/// Each page load as a `request` span (#628, #804), whatever it was served.
async fn recorded(request: Request, next: Next) -> Response {
    let span = configurator::telemetry::request(
        request.uri().path(),
        request.headers().contains_key("hx-request"),
    );
    let response = next.run(request).instrument(span.clone()).await;
    configurator::telemetry::request_ended(&span, response.status().is_success());
    response
}

/// The equipment, read afresh from the store, with the last post if it was
/// refused.
async fn equipment_shown(runs: &Runs, refused: Option<&equipment::Refused<'_>>) -> Markup {
    let store = &runs.setting().store;
    let facts = configurator::equipment::facts(store).await;
    match facts {
        Ok(facts) => equipment::page(
            &facts,
            configurator::equipment::today(&runs.setting().zone),
            refused,
        ),
        Err(error) => equipment::unreadable(&error.to_string()),
    }
}

/// Body weight's and body composition's own pages, which are now one (#786),
/// over the same range.
async fn to_body(RawQuery(query): RawQuery) -> Redirect {
    Redirect::permanent(&query.map_or_else(
        || body::PATH.to_owned(),
        |query| format!("{}?{query}", body::PATH),
    ))
}

async fn equipment_page(State(runs): State<Arc<Runs>>) -> Markup {
    equipment_shown(&runs, None).await
}

/// Declare, change or correct a fact, then show the page again: redirected on
/// success, so a reload does not post it twice, and rendered with the reason on
/// a refusal, as typed and with the reason beside the field it is about.
async fn equipment_post(
    State(runs): State<Arc<Runs>>,
    Form(form): Form<Vec<(String, String)>>,
) -> Response {
    let refusal = match equipment::asked(&form) {
        Err(refusal) => Some(refusal),
        Ok(asked) => {
            let store = &runs.setting().store;
            let done = match asked {
                equipment::Asked::Declare(statement, from) => {
                    configurator::equipment::declare(store, &statement, from).await
                }
                equipment::Asked::Correct(fact, statement) => {
                    configurator::equipment::correct(store, fact, &statement).await
                }
            };
            done.err().map(|error| equipment::refusal(&error))
        }
    };
    match refusal {
        None => Redirect::to("/equipment").into_response(),
        Some(refusal) => equipment_shown(
            &runs,
            Some(&equipment::Refused {
                typed: &form,
                refusal: &refusal,
            }),
        )
        .await
        .into_response(),
    }
}

/// The schedule, read afresh, at the week commencing `week` or as tapped.
async fn schedule_shown(
    runs: &Runs,
    week: Option<jiff::civil::Date>,
    tapped: Option<&schedule::Tapped>,
    refused: Option<&input::Refusal>,
) -> Markup {
    let store = &runs.setting().store;
    let diary = configurator::scheduling::diary(store).await;
    let today = configurator::equipment::today(&runs.setting().zone);
    let monday = domain::planner::commencing(week.unwrap_or(today));
    match diary {
        Ok(diary) => schedule::page(&diary, today, monday, tapped, refused),
        Err(error) => schedule::unreadable(&error.to_string()),
    }
}

async fn schedule_page(
    State(runs): State<Arc<Runs>>,
    Query(query): Query<HashMap<String, String>>,
) -> Markup {
    let week = query.get("week").and_then(|typed| typed.parse().ok());
    schedule_shown(&runs, week, None, None).await
}

/// Tap, clear or save the week, or start or end an illness, then show the
/// page again: redirected on a save, so a reload does not post it twice, and
/// drawn as tapped otherwise.
async fn schedule_post(
    State(runs): State<Arc<Runs>>,
    Form(form): Form<Vec<(String, String)>>,
) -> Response {
    let asked = match schedule::asked(&form) {
        Ok(asked) => asked,
        Err(refusal) => {
            return schedule_shown(&runs, None, None, Some(&refusal))
                .await
                .into_response();
        }
    };
    let tapped = match &asked {
        schedule::Asked::Redrawn(tapped) => {
            return schedule_shown(&runs, Some(tapped.monday), Some(tapped), None)
                .await
                .into_response();
        }
        schedule::Asked::ThisWeek(tapped) | schedule::Asked::FromNowOn(tapped) => Some(tapped),
        schedule::Asked::Ill(_) | schedule::Asked::Better => None,
    };
    let store = &runs.setting().store;
    let today = configurator::equipment::today(&runs.setting().zone);
    let done = match &asked {
        schedule::Asked::ThisWeek(tapped) => configurator::scheduling::alter_week(
            store,
            tapped.monday,
            &tapped.slots,
            &tapped.reason,
        )
        .await
        .map(drop),
        schedule::Asked::FromNowOn(tapped) => {
            configurator::scheduling::from_week(store, tapped.monday, tapped.slots.clone())
                .await
                .map(drop)
        }
        schedule::Asked::Ill(from) => configurator::scheduling::ill(store, *from).await.map(drop),
        schedule::Asked::Better | schedule::Asked::Redrawn(_) => {
            configurator::scheduling::better(store, today)
                .await
                .map(drop)
        }
    };
    let refused = done.err().map(|error| schedule::refusal(&error));
    let week = tapped.map(|tapped| tapped.monday);
    match refused {
        None => Redirect::to(&week.map_or_else(
            || schedule::HERE.to_owned(),
            |week| format!("{}?week={week}", schedule::HERE),
        ))
        .into_response(),
        Some(refusal) => schedule_shown(&runs, week, tapped, Some(&refusal))
            .await
            .into_response(),
    }
}

/// One workout as it runs today, read afresh, with the last post if it was
/// refused.
async fn workout_shown(
    runs: &Runs,
    intensity: Relative,
    refused: Option<&workout::Refused<'_>>,
) -> Markup {
    let store = &runs.setting().store;
    let zone = &runs.setting().zone;
    let today = configurator::equipment::today(zone);
    let found = configurator::fills::on(store, zone, today).await;
    let in_force = configurator::design::in_force(store).await;
    let facts = configurator::equipment::facts(store).await;
    match (found, in_force, facts) {
        (Ok(found), Ok(in_force), Ok(facts)) => workout::page(
            &workout::Shown {
                intensity,
                found: found.as_ref(),
                in_force: &in_force,
                facts: &facts,
                today,
            },
            refused,
        ),
        (Err(error), _, _) => workout::unreadable(&error.to_string()),
        (_, Err(error), _) => workout::unreadable(&error.to_string()),
        (_, _, Err(error)) => workout::unreadable(&error.to_string()),
    }
}

/// The workout a query or a post names, the higher-intensity one if none.
fn intensity_of<'a>(mut pairs: impl Iterator<Item = (&'a String, &'a String)>) -> Relative {
    pairs
        .find(|(key, _)| *key == "intensity")
        .and_then(|(_, value)| Relative::try_from(value.clone()).ok())
        .unwrap_or(Relative::Higher)
}

async fn workout_page(
    State(runs): State<Arc<Runs>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Markup {
    workout_shown(&runs, intensity_of(query.iter()), None).await
}

/// Change a slot or author a progression, then show the workout again,
/// keeping what was typed when it is refused.
async fn workout_post(
    State(runs): State<Arc<Runs>>,
    Form(form): Form<Vec<(String, String)>>,
) -> Response {
    let intensity = intensity_of(form.iter().map(|(key, value)| (key, value)));
    let refusal = workout_written(&runs.setting().store, &runs.setting().zone, &form).await;
    match refusal {
        None => Redirect::to(&format!(
            "{}?intensity={}",
            workout::HERE,
            intensity.as_str()
        ))
        .into_response(),
        Some(refusal) => workout_shown(
            &runs,
            intensity,
            Some(&workout::Refused {
                typed: &form,
                refusal: &refusal,
            }),
        )
        .await
        .into_response(),
    }
}

/// Write what a post asks, or why it was refused.
async fn workout_written(
    store: &configurator::store::Store,
    zone: &domain::normalised::OperatorZone,
    form: &[(String, String)],
) -> Option<input::Refusal> {
    let whole = |error: &dyn std::fmt::Display| Some(input::Refusal::whole(error.to_string()));
    if form.iter().any(|(key, _)| key == "design") {
        let done = match progression::asked(&progression::Fields::new(form.to_vec())) {
            Err(refused) => return Some(input::Refusal::whole(refused)),
            Ok(progression::Asked::Linear(linear)) => {
                configurator::design::linear(store, &linear).await
            }
            Ok(progression::Asked::Sbs(sbs)) => configurator::design::sbs(store, &sbs).await,
        };
        return done.err().and_then(|error| whole(&error));
    }
    let asked = match workout::asked(form) {
        Ok(asked) => asked,
        Err(refusal) => return Some(refusal),
    };
    let today = configurator::equipment::today(zone);
    let found = match configurator::fills::on(store, zone, today).await {
        Ok(Some(found)) => found,
        Ok(None) => return whole(&"no mesocycle is in force today"),
        Err(error) => return whole(&error),
    };
    let in_force = match configurator::design::in_force(store).await {
        Ok(in_force) => in_force,
        Err(error) => return whole(&error),
    };
    let writes = match workout::writes(&asked, &found, &in_force) {
        Ok(writes) => writes,
        Err(refusal) => return Some(refusal),
    };
    if let Some(change) = writes.change
        && let Err(error) = configurator::fills::change(store, zone, change).await
    {
        return Some(workout::refusal(&error));
    }
    for (intensity, version) in writes.workouts {
        if let Err(error) = configurator::design::workout(store, intensity, &version).await {
            return whole(&error);
        }
    }
    None
}

/// Every change to the run, starting with how it stands now.
async fn events(
    State(runs): State<Arc<Runs>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let changes = WatchStream::new(runs.watch()).map(|view| {
        Ok(Event::default()
            .event("run")
            .data(view.render().into_string()))
    });
    Sse::new(changes).keep_alive(KeepAlive::default())
}

/// The step the run is on, for a page drawn from the store, as `ended` once a
/// run has ended since `since` and as `step` otherwise; and each part
/// collected after the first `collected`, which the cards waiting on it ask
/// to be drawn again on (#830).
async fn step(
    State(runs): State<Arc<Runs>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let count = |name: &str, otherwise: u64| {
        query
            .get(name)
            .and_then(|count| count.parse().ok())
            .unwrap_or(otherwise)
    };
    let view = runs.view();
    let mut since = count("since", view.ended());
    let mut collected = count("collected", view.collected());
    let changes = WatchStream::new(runs.watch()).map(move |view| {
        run::page_events(&mut since, &mut collected, &view)
            .into_iter()
            .map(|(name, data)| Ok(Event::default().event(name).data(data)))
            .collect::<Vec<_>>()
    });
    Sse::new(Each::new(changes)).keep_alive(KeepAlive::default())
}

/// Each item of each batch a stream yields, in order.
///
/// **By hand**, because `tokio-stream` has no `flat_map` and one change to
/// the run can be several events.
struct Each<S, T> {
    batches: S,
    queued: std::collections::VecDeque<T>,
}

impl<S, T> Each<S, T> {
    const fn new(batches: S) -> Self {
        Self {
            batches,
            queued: std::collections::VecDeque::new(),
        }
    }
}

impl<S: Stream<Item = Vec<T>> + Unpin, T: Unpin> Stream for Each<S, T> {
    type Item = T;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<T>> {
        loop {
            if let Some(item) = self.queued.pop_front() {
                return std::task::Poll::Ready(Some(item));
            }
            match std::pin::Pin::new(&mut self.batches).poll_next(context) {
                std::task::Poll::Ready(Some(batch)) => self.queued.extend(batch),
                std::task::Poll::Ready(None) => return std::task::Poll::Ready(None),
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        }
    }
}

/// htmx 2.0.11 and its server-sent events extension 2.2.4, vendored (§ 20):
/// driven entirely by attributes, so no application script is authored.
async fn htmx() -> impl IntoResponse {
    script(include_str!("../assets/htmx.min.js"))
}

async fn sse() -> impl IntoResponse {
    script(include_str!("../assets/sse.min.js"))
}

/// The one stylesheet every page links (#614).
async fn stylesheet() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../assets/style.css"),
    )
}

fn script(body: &'static str) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        body,
    )
}

#[cfg(test)]
mod tests {
    use domain::normalised::OperatorZone;

    use crate::run::{Runs, Setting};

    /// **No drawn page reads what has no version** (#805), so each is served
    /// as it was drawn while nothing it reads has moved, rather than drawn
    /// every time it is opened.
    #[test]
    fn every_drawn_page_reads_only_what_has_a_version() {
        let graph = configurator::graph::graph();
        for (route, reads) in super::pages().reads() {
            let unversioned: Vec<_> = reads
                .nodes()
                .filter(|node| application::drawing::unversioned(graph, node))
                .collect();
            assert!(unversioned.is_empty(), "{route} reads {unversioned:?}");
        }
    }

    /// **A weigh-in redraws the cards that read body weight, on every page
    /// they appear on, and no other card** (#830): each card is drawn again
    /// only when a node upstream of what it reads has moved, and a weigh-in
    /// moves Withings's normalised layer and everything derived from it.
    ///
    /// Pulse wave velocity is a part of the Body Scan's weigh-in, so its cards
    /// are among them. The FTP tests and the 1RM estimates only consult body
    /// weight, since each test is read against the weigh-in on its own day:
    /// their cards are not.
    #[test]
    fn a_weigh_in_redraws_only_the_cards_reading_body_weight() {
        let graph = configurator::graph::graph();
        let weighed = application::graph::Node::Normalised(
            domain::landing::LandingStream::try_from("withings.measurements").expect("a stream"),
        );
        let redrawn: Vec<&str> = super::pages()
            .cards_read()
            .iter()
            .filter(|(_, reads)| graph.moved_by(reads).contains(&weighed))
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            redrawn,
            [
                "body-composition-charts",
                "body-weigh-ins",
                "body-weight-charts",
                "pulse-wave-velocity-latest",
                "pulse-wave-velocity-readings",
                "pulse-wave-velocity-summary",
                "summary-body-weight",
                "summary-fat",
                "summary-fat-free",
            ]
        );
    }

    /// **A night of HRV redraws the HRV cards and no other** (#830): its
    /// response is a node of its own, so resting heart rate and stress, on
    /// Progress and on /working, are served as they were.
    #[test]
    fn a_night_of_hrv_redraws_only_the_cards_reading_hrv() {
        let graph = configurator::graph::graph();
        let night = application::graph::Node::Normalised(
            domain::landing::LandingStream::try_from("garmin.hrv").expect("a stream"),
        );
        let redrawn: Vec<&str> = super::pages()
            .cards_read()
            .iter()
            .filter(|(_, reads)| graph.moved_by(reads).contains(&night))
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            redrawn,
            [
                "hrv-latest",
                "hrv-nights",
                "summary-overnight-hrv",
                "working-each-week",
                "working-so-far",
            ]
        );
    }

    /// **A gym session made canonical redraws the home page's cards** (#830):
    /// what is next and this microcycle are read from where the plan stands,
    /// which the sessions performed move.
    #[test]
    fn a_canonical_gym_session_redraws_the_home_page() {
        let graph = configurator::graph::graph();
        let performed = application::graph::Node::Canonical(application::CanonicalLayer::Gym);
        let cards = super::pages();
        let redrawn: Vec<&str> = cards
            .cards_read()
            .iter()
            .filter(|(name, reads)| {
                name.starts_with("home-") && graph.moved_by(reads).contains(&performed)
            })
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(redrawn, ["home-curves", "home-microcycle", "home-next"]);
    }

    /// A span as it was opened: its name, the fields that say what it is,
    /// and the span it is under or follows from.
    #[derive(Debug, Clone, Default)]
    struct Opened {
        name: &'static str,
        fields: std::collections::BTreeMap<String, String>,
        under: Option<tracing::span::Id>,
    }

    /// Every span opened, by id.
    #[derive(Clone, Default)]
    struct Opening(std::sync::Arc<std::sync::Mutex<std::collections::HashMap<u64, Opened>>>);

    struct Fields<'a>(&'a mut std::collections::BTreeMap<String, String>);

    impl tracing::field::Visit for Fields<'_> {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0.insert(
                field.name().to_owned(),
                format!("{value:?}").replace('"', ""),
            );
        }
    }

    impl<S> tracing_subscriber::Layer<S> for Opening
    where
        S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
    {
        fn on_new_span(
            &self,
            attributes: &tracing::span::Attributes<'_>,
            id: &tracing::span::Id,
            context: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut opened = Opened {
                name: attributes.metadata().name(),
                under: context
                    .span(id)
                    .and_then(|span| span.parent().map(|parent| parent.id())),
                ..Opened::default()
            };
            attributes.record(&mut Fields(&mut opened.fields));
            if let Ok(mut opening) = self.0.lock() {
                opening.insert(id.into_u64(), opened);
            }
        }

        fn on_record(
            &self,
            id: &tracing::span::Id,
            values: &tracing::span::Record<'_>,
            _: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if let Ok(mut opening) = self.0.lock()
                && let Some(opened) = opening.get_mut(&id.into_u64())
            {
                values.record(&mut Fields(&mut opened.fields));
            }
        }

        fn on_follows_from(
            &self,
            id: &tracing::span::Id,
            follows: &tracing::span::Id,
            _: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if let Ok(mut opening) = self.0.lock()
                && let Some(opened) = opening.get_mut(&id.into_u64())
                && opened.under.is_none()
            {
                opened.under = Some(follows.clone());
            }
        }
    }

    /// **Every route is recorded, listed or not** (#804): the equipment page
    /// and a page that does not exist were never on the list the chart pages
    /// were, and each is a `request`. **A drawn page's parts are its
    /// children**: each read, rendering, keeping it, and on a chart's page,
    /// checking whether a run is due. **None of them is opening the store.**
    #[test]
    fn every_page_load_is_recorded_with_a_span_for_each_part() {
        use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
        use tracing_subscriber::layer::SubscriberExt as _;

        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        // Opened before anything is recorded, as `serve` opens it before it
        // serves anything.
        let runs = std::sync::Arc::new(Runs::new(Setting {
            store: runtime
                .block_on(configurator::store::Store::open(
                    &directory.path().join("fitness.db"),
                ))
                .expect("the store"),
            zone: OperatorZone::try_from("Europe/London").expect("a zone"),
            credentials: None,
            build: None,
        }));
        let opening = Opening::default();
        let _recording =
            tracing::subscriber::set_default(tracing_subscriber::registry().with(opening.clone()));
        runtime.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("a port");
            let address = listener.local_addr().expect("an address");
            tokio::spawn(async move { axum::serve(listener, super::app(runs)).await });
            for path in ["/equipment", "/nowhere", "/history/2026-11-02/1", "/hrv"] {
                let mut stream = tokio::net::TcpStream::connect(address)
                    .await
                    .expect("a connection");
                stream
                    .write_all(
                        format!("GET {path} HTTP/1.1\r\nHost: here\r\nConnection: close\r\n\r\n")
                            .as_bytes(),
                    )
                    .await
                    .expect("a request");
                let mut response = Vec::new();
                stream.read_to_end(&mut response).await.expect("a response");
            }
        });

        let opened = opening.0.lock().expect("unpoisoned").clone();
        let request = |page: &str| {
            opened
                .iter()
                .find(|(_, opened)| opened.name == "request" && opened.fields["page"] == page)
                .map_or_else(|| panic!("{page} is recorded: {opened:?}"), |(id, _)| *id)
        };
        request("/equipment");
        request("/nowhere");
        let parts = |page: &str| -> Vec<String> {
            let load = request(page);
            opened
                .values()
                .filter(|step| {
                    step.name == "step"
                        && step.under.as_ref().map(tracing::span::Id::into_u64) == Some(load)
                })
                .map(|step| {
                    step.fields.get("read").map_or_else(
                        || step.fields["step"].clone(),
                        |read| format!("{} {read}", step.fields["step"]),
                    )
                })
                .collect()
        };
        let session = parts("/history/2026-11-02/1");
        for part in ["reading opened", "rendering", "keeping"] {
            assert!(
                session.iter().any(|seen| seen == part),
                "{part}: {session:?}"
            );
        }
        let chart = parts("/hrv");
        assert!(chart.iter().any(|seen| seen == "checking"), "{chart:?}");
        // **No page load opens the store** (#663), neither drawing the page nor
        // checking whether a run is due: it was opened once, before serving.
        let openings: Vec<_> = opened
            .values()
            .filter(|step| {
                step.name == "step"
                    && step
                        .fields
                        .get("step")
                        .is_some_and(|step| step == "opening")
            })
            .collect();
        assert!(openings.is_empty(), "{} openings", openings.len());
    }
}
