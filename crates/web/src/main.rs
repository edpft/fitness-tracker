//! The HTTP driving adapter: `next` from a browser (#398).
//!
//! One page with one action. Pressing it starts a run of `next`; the page
//! follows the run's steps as they end, pushed as server-sent events, and ends
//! on the report the terminal prints. The use case comes from the
//! configurator, so this crate requests, renders and interacts, and names no
//! adapter.

mod chart;
mod equipment;
mod one_rep_max;
mod page;
mod report;
mod run;
mod weight;

use std::{convert::Infallible, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Form, Router,
    extract::{Query, State},
    http::header,
    response::{
        IntoResponse, Redirect, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use clap::{Arg, ArgMatches, Command, value_parser};
use maud::Markup;
use tokio_stream::{Stream, StreamExt as _, wrappers::WatchStream};

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
    let served = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())
        .and_then(|runtime| runtime.block_on(serve(&matches)));
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
async fn serve(matches: &ArgMatches) -> Result<(), String> {
    let database = match matches.get_one::<PathBuf>("database") {
        Some(path) => path.clone(),
        None => configurator::paths::store(&configurator::paths::SystemEnvironment)
            .map_err(|error| error.to_string())?,
    };
    let stated = configurator::config::stated_timezone(&database)
        .await
        .map_err(|error| error.to_string())?;
    let zone = configurator::config::timezone(
        matches.get_one::<String>("timezone").map(String::as_str),
        stated.as_deref(),
    )
    .map_err(|error| error.to_string())?;
    let credentials = configurator::config::credentials().map_err(|error| error.to_string())?;

    let runs = Arc::new(Runs::new(Setting {
        database,
        zone,
        credentials,
    }));
    let app = Router::new()
        .route("/", get(index))
        .route("/heart-rate", get(heart_rate))
        .route("/body-weight", get(body_weight))
        .route("/estimated-1rm", get(estimated_one_rep_max))
        .route("/equipment", get(equipment_page).post(equipment_post))
        .route("/run", post(start))
        .route("/run/events", get(events))
        .route("/htmx.min.js", get(htmx))
        .route("/sse.min.js", get(sse))
        .with_state(runs);

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

async fn index(State(runs): State<Arc<Runs>>) -> Markup {
    page::index(&runs.view())
}

/// Every main ride's heart rate per power zone, read afresh from the store.
async fn heart_rate(State(runs): State<Arc<Runs>>) -> Markup {
    let setting = runs.setting();
    let store = match configurator::store::Store::open(&setting.database).await {
        Ok(store) => store,
        Err(error) => return chart::unreadable(&error.to_string()),
    };
    let rides = configurator::heart_rate::rides(&store, &setting.zone).await;
    store.close().await;
    match rides {
        Ok(rides) => chart::page(&rides),
        Err(error) => chart::unreadable(&error.to_string()),
    }
}

/// Body weight over a range, read afresh from the store.
async fn body_weight(
    State(runs): State<Arc<Runs>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Markup {
    let over = weight::Over::named(query.get("over").map(String::as_str));
    let store = match configurator::store::Store::open(&runs.setting().database).await {
        Ok(store) => store,
        Err(error) => return weight::unreadable(&error.to_string()),
    };
    let view = configurator::body_weight::view(&store, &runs.setting().zone).await;
    store.close().await;
    match view {
        Ok(view) => weight::page(&view, over),
        Err(error) => weight::unreadable(&error.to_string()),
    }
}

/// Estimated 1RM in each planned macrocycle, read afresh from the store.
async fn estimated_one_rep_max(State(runs): State<Arc<Runs>>) -> Markup {
    let store = match configurator::store::Store::open(&runs.setting().database).await {
        Ok(store) => store,
        Err(error) => return one_rep_max::unreadable(&error.to_string()),
    };
    let view = configurator::one_rep_max::view(&store, &runs.setting().zone).await;
    store.close().await;
    match view {
        Ok(view) => one_rep_max::page(&view),
        Err(error) => one_rep_max::unreadable(&error.to_string()),
    }
}

/// The equipment, read afresh from the store, with what the last post came to.
async fn equipment_shown(runs: &Runs, said: Option<&str>) -> Markup {
    let store = match configurator::store::Store::open(&runs.setting().database).await {
        Ok(store) => store,
        Err(error) => return equipment::unreadable(&error.to_string()),
    };
    let facts = configurator::equipment::facts(&store).await;
    store.close().await;
    match facts {
        Ok(facts) => equipment::page(
            &facts,
            configurator::equipment::today(&runs.setting().zone),
            said,
        ),
        Err(error) => equipment::unreadable(&error.to_string()),
    }
}

async fn equipment_page(State(runs): State<Arc<Runs>>) -> Markup {
    equipment_shown(&runs, None).await
}

/// Declare, change or correct a fact, then show the page again: redirected on
/// success, so a reload does not post it twice, and rendered with the reason on
/// a refusal, so what was typed is not the thing lost.
async fn equipment_post(
    State(runs): State<Arc<Runs>>,
    Form(form): Form<Vec<(String, String)>>,
) -> Response {
    let refused = match equipment::asked(&equipment::Fields::new(form)) {
        Err(refused) => Some(refused),
        Ok(asked) => match configurator::store::Store::open(&runs.setting().database).await {
            Err(error) => Some(error.to_string()),
            Ok(store) => {
                let done = match asked {
                    equipment::Asked::Declare(statement, from) => {
                        configurator::equipment::declare(&store, &statement, from).await
                    }
                    equipment::Asked::Correct(fact, statement) => {
                        configurator::equipment::correct(&store, fact, &statement).await
                    }
                };
                store.close().await;
                done.err().map(|error| error.to_string())
            }
        },
    };
    match refused {
        None => Redirect::to("/equipment").into_response(),
        Some(refused) => equipment_shown(&runs, Some(&refused)).await.into_response(),
    }
}

/// Start a run, answering with what the page should say about starting it:
/// nothing, or why not.
async fn start(State(runs): State<Arc<Runs>>) -> Markup {
    page::notice(runs.start().err().as_deref())
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

/// htmx 2.0.11 and its server-sent events extension 2.2.4, vendored (§ 20):
/// driven entirely by attributes, so no application script is authored.
async fn htmx() -> impl IntoResponse {
    script(include_str!("../assets/htmx.min.js"))
}

async fn sse() -> impl IntoResponse {
    script(include_str!("../assets/sse.min.js"))
}

fn script(body: &'static str) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        body,
    )
}
