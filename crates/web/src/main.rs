//! The HTTP driving adapter: `next` from a browser (#398).
//!
//! One page with one action. Pressing it starts a run of `next`; the page
//! follows the run's steps as they end, pushed as server-sent events, and ends
//! on the report the terminal prints. The use case comes from the
//! configurator, so this crate requests, renders and interacts, and names no
//! adapter.

mod devices;
mod page;
mod report;
mod run;

use std::{convert::Infallible, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Router,
    extract::State,
    http::header,
    response::{
        IntoResponse,
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

    let store = configurator::store::Store::open(&database)
        .await
        .map_err(|error| error.to_string())?;
    let register = Arc::new(devices::Register {
        store,
        zone: zone.clone(),
    });
    let runs = Arc::new(Runs::new(Setting {
        database,
        zone,
        credentials,
    }));
    let app = Router::new()
        .route("/", get(index))
        .route("/run", post(start))
        .route("/run/events", get(events))
        .route("/htmx.min.js", get(htmx))
        .route("/sse.min.js", get(sse))
        .with_state(runs)
        .merge(devices::routes(register));

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
