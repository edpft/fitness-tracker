//! The HTTP driving adapter: `next` from a browser (#398).
//!
//! Opening a page starts a run of `next` (#466); the front page follows the
//! run's steps as they end, pushed as server-sent events, and ends on the
//! report the terminal prints, and a chart redraws from the store when it ends. The use case comes from the
//! configurator, so this crate requests, renders and interacts, and names no
//! adapter.

mod chart;
mod composition;
mod daily;
mod equipment;
mod fills;
mod ftp;
mod one_rep_max;
mod page;
mod pulse_wave_velocity;
mod range;
mod report;
mod run;
mod sleep;
mod weight;
mod workouts;

use std::{collections::HashMap, convert::Infallible, net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Form, Router,
    extract::{Query, State},
    http::{HeaderMap, header},
    response::{
        IntoResponse, Redirect, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::get,
};
use clap::{Arg, ArgMatches, Command, value_parser};
use maud::Markup;
use tokio_stream::{Stream, StreamExt as _, wrappers::WatchStream};

use run::{Asked, Runs, Setting};

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
    // **Where they are kept, not what was in them** (#438): each run reads
    // them, so `fitness credentials` takes effect without a restart.
    let credentials =
        configurator::paths::credentials(&configurator::paths::SystemEnvironment).ok();

    let runs = Arc::new(Runs::new(Setting {
        database,
        zone,
        credentials,
    }));
    let app = Router::new()
        .route("/", get(index))
        .route("/heart-rate", get(heart_rate))
        .route(ftp::PATH, get(ftp_tests))
        .route("/body-weight", get(body_weight))
        .route("/body-composition", get(body_composition))
        .route(pulse_wave_velocity::PATH, get(pulse_wave_velocity_page))
        .route("/hrv", get(overnight_hrv))
        .route("/resting-heart-rate", get(resting_heart_rate))
        .route("/sleep", get(sleep_page))
        .route("/estimated-1rm", get(estimated_one_rep_max))
        .route("/equipment", get(equipment_page).post(equipment_post))
        .route("/fills", get(fills_page).post(fills_post))
        .route("/workouts", get(workouts_page).post(workouts_post))
        .route("/run/events", get(events))
        .route("/run/step", get(step))
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

async fn index(State(runs): State<Arc<Runs>>, headers: HeaderMap) -> Markup {
    page::index(&opened(&runs, &headers, Ok(Asked::Everything)))
}

/// Start a run of what a page asks for as it is opened, or join one going
/// that covers it, and say where it has got to.
///
/// **Not when htmx asks**, which is a page redrawing itself because a run
/// ended: starting another would redraw it again when that one ended, for
/// ever.
fn opened(runs: &Arc<Runs>, headers: &HeaderMap, asked: Result<Asked, String>) -> run::View {
    if !headers.contains_key("hx-request") {
        match asked {
            Ok(asked) => runs.start(&asked),
            Err(error) => runs.refuse(&format!("the run could not be asked for: {error}")),
        }
    }
    runs.view()
}

/// What a chart's page asks a run to collect: what it reads.
fn only(
    collects: &Result<application::next::Scope, configurator::wiring::WiringError>,
) -> Result<Asked, String> {
    collects
        .as_ref()
        .map(|scope| Asked::Only(scope.clone()))
        .map_err(ToString::to_string)
}

/// A chart's page drawn from the store, or while a run is collecting what it
/// reads, the page as it was last drawn, and the placeholder where it has not
/// been (#467).
///
/// **Asked before the store is read and again after**, so a run that starts
/// while the page is being drawn is not missed: the read is one snapshot, but
/// one taken after a layer was emptied and before it was rebuilt.
async fn drawn(
    runs: &Runs,
    collects: &Result<application::next::Scope, configurator::wiring::WiringError>,
    here: &str,
    over: Option<domain::analytical::range::Over>,
    run: &run::View,
    read: impl Future<Output = Markup>,
) -> Markup {
    let Ok(scope) = collects else {
        return read.await;
    };
    let page = format!("{here} {over:?}");
    let meanwhile = || {
        runs.kept(&page)
            .map_or_else(|| page::collecting(here, run), |kept| page::kept(&kept))
    };
    if runs.collecting(scope) {
        return meanwhile();
    }
    let drawn = read.await;
    if runs.collecting(scope) {
        meanwhile()
    } else {
        runs.keep(&page, &drawn);
        drawn
    }
}

/// The range `?over=` names.
fn over(query: &Query<HashMap<String, String>>) -> domain::analytical::range::Over {
    range::named(query.get("over").map(String::as_str))
}

/// Every main ride's heart rate per power zone over a range, read afresh from
/// the store.
async fn heart_rate(
    State(runs): State<Arc<Runs>>,
    query: Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Markup {
    let collects = configurator::heart_rate::collects();
    let run = opened(&runs, &headers, only(&collects));
    drawn(
        &runs,
        &collects,
        "/heart-rate",
        Some(over(&query)),
        &run,
        async {
            let setting = runs.setting();
            let store = match configurator::store::Store::open(&setting.database).await {
                Ok(store) => store,
                Err(error) => return chart::unreadable(&error.to_string()),
            };
            let rides = configurator::heart_rate::rides(&store).await;
            let ranges = configurator::ranges::today(&store, &setting.zone).await;
            store.close().await;
            match rides.and_then(|rides| Ok((rides, ranges?))) {
                Ok((rides, ranges)) => chart::page(&rides, &ranges, over(&query), &run),
                Err(error) => chart::unreadable(&error.to_string()),
            }
        },
    )
    .await
}

/// The FTP tests over a range, in watts and over body weight, read afresh
/// from the store (#469).
async fn ftp_tests(
    State(runs): State<Arc<Runs>>,
    query: Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Markup {
    let collects = configurator::ftp_tests::collects();
    let run = opened(&runs, &headers, only(&collects));
    drawn(
        &runs,
        &collects,
        ftp::PATH,
        Some(over(&query)),
        &run,
        async {
            let setting = runs.setting();
            let store = match configurator::store::Store::open(&setting.database).await {
                Ok(store) => store,
                Err(error) => return ftp::unreadable(&error.to_string()),
            };
            let tests = configurator::ftp_tests::tests(&store).await;
            let ranges = configurator::ranges::today(&store, &setting.zone).await;
            store.close().await;
            match tests.and_then(|tests| Ok((tests, ranges?))) {
                Ok((tests, ranges)) => ftp::page(&tests, &ranges, over(&query), &run),
                Err(error) => ftp::unreadable(&error.to_string()),
            }
        },
    )
    .await
}

/// Body weight over a range, read afresh from the store.
async fn body_weight(
    State(runs): State<Arc<Runs>>,
    query: Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Markup {
    let collects = configurator::body_weight::collects();
    let run = opened(&runs, &headers, only(&collects));
    drawn(
        &runs,
        &collects,
        "/body-weight",
        Some(over(&query)),
        &run,
        async {
            let setting = runs.setting();
            let store = match configurator::store::Store::open(&setting.database).await {
                Ok(store) => store,
                Err(error) => return weight::unreadable(&error.to_string()),
            };
            let series = configurator::body_weight::series(&store).await;
            let ranges = configurator::ranges::today(&store, &setting.zone).await;
            store.close().await;
            match series.and_then(|series| Ok((series, ranges?))) {
                Ok((series, ranges)) => weight::page(&series, &ranges, over(&query), &run),
                Err(error) => weight::unreadable(&error.to_string()),
            }
        },
    )
    .await
}

/// Body composition over a range, one series per method, read afresh from the
/// store.
async fn body_composition(
    State(runs): State<Arc<Runs>>,
    query: Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Markup {
    let collects = configurator::composition::collects();
    let run = opened(&runs, &headers, only(&collects));
    drawn(
        &runs,
        &collects,
        "/body-composition",
        Some(over(&query)),
        &run,
        async {
            let setting = runs.setting();
            let store = match configurator::store::Store::open(&setting.database).await {
                Ok(store) => store,
                Err(error) => return composition::unreadable(&error.to_string()),
            };
            let components = configurator::composition::by_method(&store).await;
            let against = configurator::composition::against_phase(&store, &setting.zone).await;
            let ranges = configurator::ranges::today(&store, &setting.zone).await;
            store.close().await;
            match components.and_then(|components| Ok((components, against?, ranges?))) {
                Ok((components, against, ranges)) => {
                    composition::page(&components, &against, &ranges, over(&query), &run)
                }
                Err(error) => composition::unreadable(&error.to_string()),
            }
        },
    )
    .await
}

/// Overnight HRV over a range, one series per watch and the trend through
/// each, read afresh from the store.
async fn overnight_hrv(
    State(runs): State<Arc<Runs>>,
    query: Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Markup {
    let collects = configurator::per_device::overnight_hrv_collects();
    let run = opened(&runs, &headers, only(&collects));
    drawn(&runs, &collects, "/hrv", Some(over(&query)), &run, async {
        let shown = daily::OVERNIGHT_HRV;
        let setting = runs.setting();
        let store = match configurator::store::Store::open(&setting.database).await {
            Ok(store) => store,
            Err(error) => return daily::unreadable(shown, &error.to_string()),
        };
        let trended = configurator::per_device::overnight_hrv_trend(&store).await;
        let ranges = configurator::ranges::today(&store, &setting.zone).await;
        store.close().await;
        match trended.and_then(|trended| Ok((trended, ranges?))) {
            Ok((trended, ranges)) => daily::overnight_hrv(&trended, &ranges, over(&query), &run),
            Err(error) => daily::unreadable(shown, &error.to_string()),
        }
    })
    .await
}

/// Pulse wave velocity over a range, with its slow line, read afresh from the
/// store (#471).
async fn pulse_wave_velocity_page(
    State(runs): State<Arc<Runs>>,
    query: Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Markup {
    let collects = configurator::pulse_wave_velocity::collects();
    let run = opened(&runs, &headers, only(&collects));
    drawn(
        &runs,
        &collects,
        pulse_wave_velocity::PATH,
        Some(over(&query)),
        &run,
        async {
            let setting = runs.setting();
            let store = match configurator::store::Store::open(&setting.database).await {
                Ok(store) => store,
                Err(error) => return pulse_wave_velocity::unreadable(&error.to_string()),
            };
            let series = configurator::pulse_wave_velocity::by_algorithm(&store).await;
            let ranges = configurator::ranges::today(&store, &setting.zone).await;
            store.close().await;
            match series.and_then(|series| Ok((series, ranges?))) {
                Ok((series, ranges)) => {
                    pulse_wave_velocity::page(&series, &ranges, over(&query), &run)
                }
                Err(error) => pulse_wave_velocity::unreadable(&error.to_string()),
            }
        },
    )
    .await
}

/// Resting heart rate over a range, one series per watch with its slow line
/// and the nights it flags, read afresh from the store.
async fn resting_heart_rate(
    State(runs): State<Arc<Runs>>,
    query: Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Markup {
    let collects = configurator::per_device::resting_heart_rate_collects();
    let run = opened(&runs, &headers, only(&collects));
    drawn(
        &runs,
        &collects,
        "/resting-heart-rate",
        Some(over(&query)),
        &run,
        async {
            let shown = daily::RESTING_HEART_RATE;
            let setting = runs.setting();
            let store = match configurator::store::Store::open(&setting.database).await {
                Ok(store) => store,
                Err(error) => return daily::unreadable(shown, &error.to_string()),
            };
            let trended = configurator::per_device::resting_heart_rate_trend(&store).await;
            let ranges = configurator::ranges::today(&store, &setting.zone).await;
            store.close().await;
            match trended.and_then(|trended| Ok((trended, ranges?))) {
                Ok((trended, ranges)) => {
                    daily::resting_heart_rate(&trended, &ranges, over(&query), &run)
                }
                Err(error) => daily::unreadable(shown, &error.to_string()),
            }
        },
    )
    .await
}

/// A night's sleep, how long and when, one series per watch, read afresh from
/// the store (#255).
async fn sleep_page(
    State(runs): State<Arc<Runs>>,
    query: Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Markup {
    let collects = configurator::sleep::collects();
    let run = opened(&runs, &headers, only(&collects));
    drawn(
        &runs,
        &collects,
        sleep::PATH,
        Some(over(&query)),
        &run,
        async {
            let setting = runs.setting();
            let store = match configurator::store::Store::open(&setting.database).await {
                Ok(store) => store,
                Err(error) => return sleep::unreadable(&error.to_string()),
            };
            let nights = configurator::sleep::nights(&store).await;
            let ranges = configurator::ranges::today(&store, &setting.zone).await;
            store.close().await;
            match nights.and_then(|nights| Ok((nights, ranges?))) {
                Ok((nights, ranges)) => sleep::page(&nights, &ranges, over(&query), &run),
                Err(error) => sleep::unreadable(&error.to_string()),
            }
        },
    )
    .await
}

/// Estimated 1RM in each planned macrocycle, read afresh from the store.
async fn estimated_one_rep_max(State(runs): State<Arc<Runs>>, headers: HeaderMap) -> Markup {
    let collects = configurator::one_rep_max::collects();
    let run = opened(&runs, &headers, only(&collects));
    drawn(&runs, &collects, "/estimated-1rm", None, &run, async {
        let store = match configurator::store::Store::open(&runs.setting().database).await {
            Ok(store) => store,
            Err(error) => return one_rep_max::unreadable(&error.to_string()),
        };
        let view = configurator::one_rep_max::view(&store, &runs.setting().zone).await;
        store.close().await;
        match view {
            Ok(view) => one_rep_max::page(&view, &run),
            Err(error) => one_rep_max::unreadable(&error.to_string()),
        }
    })
    .await
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

/// The fills in force today, read afresh, with what the last post came to.
async fn fills_shown(runs: &Runs, said: Option<&str>) -> Markup {
    let store = match configurator::store::Store::open(&runs.setting().database).await {
        Ok(store) => store,
        Err(error) => return fills::unreadable(&error.to_string()),
    };
    let zone = &runs.setting().zone;
    let today = configurator::equipment::today(zone);
    let found = configurator::fills::on(&store, zone, today).await;
    let facts = configurator::equipment::facts(&store).await;
    store.close().await;
    match (found, facts) {
        (Ok(found), Ok(facts)) => fills::page(found.as_ref(), &facts, today, said),
        (Err(error), _) => fills::unreadable(&error.to_string()),
        (_, Err(error)) => fills::unreadable(&error.to_string()),
    }
}

async fn fills_page(State(runs): State<Arc<Runs>>) -> Markup {
    fills_shown(&runs, None).await
}

/// Change a fill, then show the page again, as the equipment page does.
async fn fills_post(
    State(runs): State<Arc<Runs>>,
    Form(form): Form<Vec<(String, String)>>,
) -> Response {
    let refused = match fills::asked(&form) {
        Err(refused) => Some(refused),
        Ok(asked) => match configurator::store::Store::open(&runs.setting().database).await {
            Err(error) => Some(error.to_string()),
            Ok(store) => {
                let done = configurator::fills::change(&store, &runs.setting().zone, asked).await;
                store.close().await;
                done.err().map(|error| error.to_string())
            }
        },
    };
    match refused {
        None => Redirect::to("/fills").into_response(),
        Some(refused) => fills_shown(&runs, Some(&refused)).await.into_response(),
    }
}

/// The versions in force, read afresh, with what the last post came to.
async fn workouts_shown(
    runs: &Runs,
    said: Option<&str>,
    typed: Option<&workouts::Fields>,
) -> Markup {
    let store = match configurator::store::Store::open(&runs.setting().database).await {
        Ok(store) => store,
        Err(error) => return workouts::unreadable(&error.to_string()),
    };
    let in_force = configurator::design::in_force(&store).await;
    store.close().await;
    match in_force {
        Ok(in_force) => workouts::page(&in_force, &runs.setting().zone, said, typed),
        Err(error) => workouts::unreadable(&error.to_string()),
    }
}

async fn workouts_page(State(runs): State<Arc<Runs>>) -> Markup {
    workouts_shown(&runs, None, None).await
}

/// Author a version, then show the page again, as the equipment page does,
/// keeping what was typed when it is refused.
async fn workouts_post(
    State(runs): State<Arc<Runs>>,
    Form(form): Form<Vec<(String, String)>>,
) -> Response {
    let typed = workouts::Fields::new(form);
    let refused = match workouts::asked(&typed) {
        Err(refused) => Some(refused),
        Ok(asked) => match configurator::store::Store::open(&runs.setting().database).await {
            Err(error) => Some(error.to_string()),
            Ok(store) => {
                let done = match asked {
                    workouts::Asked::Workout(intensity, workout) => {
                        configurator::design::workout(&store, intensity, &workout).await
                    }
                    workouts::Asked::Linear(linear) => {
                        configurator::design::linear(&store, &linear).await
                    }
                    workouts::Asked::Sbs(sbs) => configurator::design::sbs(&store, &sbs).await,
                };
                store.close().await;
                done.err().map(|error| error.to_string())
            }
        },
    };
    match refused {
        None => Redirect::to("/workouts").into_response(),
        Some(refused) => workouts_shown(&runs, Some(&refused), Some(&typed))
            .await
            .into_response(),
    }
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
/// run has ended since `since` and as `step` otherwise.
async fn step(
    State(runs): State<Arc<Runs>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut since = query
        .get("since")
        .and_then(|since| since.parse().ok())
        .unwrap_or_else(|| runs.view().ended());
    let changes = WatchStream::new(runs.watch()).map(move |view| {
        let (name, shown) = run::step_event(&mut since, &view);
        Ok(Event::default().event(name).data(shown.into_string()))
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

#[cfg(test)]
mod tests {
    use application::{Part, next::Scope};
    use domain::{landing::LandingStream, normalised::OperatorZone};
    use maud::html;

    use crate::run::{Asked, Runs, Setting, View};

    /// **A page whose data a run is collecting is drawn as it was before, and
    /// the store is not read** (#467): on 2026-10-16 the estimated 1RM page
    /// read the gym's canonical layer while a run had emptied it, and said *"No
    /// test has been recorded."* A page this server has not drawn yet is the
    /// placeholder.
    #[test]
    fn a_page_is_kept_as_it_was_while_its_data_is_collected() {
        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let fresh = || {
            Runs::new(Setting {
                database: directory.path().join("fitness.db"),
                zone: OperatorZone::try_from("Europe/London").expect("a zone"),
                credentials: None,
            })
        };
        let collects = Ok(Scope::new([Part::Normalising(
            LandingStream::try_from("hevy.workouts").expect("a stream"),
        )]));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");
        let drawn = |runs: &Runs, shown: &'static str| {
            runtime
                .block_on(super::drawn(
                    runs,
                    &collects,
                    "/estimated-1rm",
                    None,
                    &View::default(),
                    async move {
                        html! { main { p { (shown) } } }
                    },
                ))
                .into_string()
        };

        let runs = fresh();
        assert!(drawn(&runs, "Front squat 92.5kg").contains("Front squat 92.5kg"));
        runs.going_over(Asked::Everything);
        let kept = drawn(&runs, "No test has been recorded.");
        assert!(kept.contains("Front squat 92.5kg"), "{kept}");
        assert!(!kept.contains("No test"), "{kept}");
        assert!(kept.contains(r#"hx-trigger="every 2s""#), "{kept}");

        let runs = fresh();
        runs.going_over(Asked::Everything);
        let shown = drawn(&runs, "No test has been recorded.");
        assert!(
            shown.contains("Collecting what this page shows…"),
            "{shown}"
        );
        assert!(shown.contains("<title>Estimated 1RM</title>"), "{shown}");
        assert!(!shown.contains("No test"), "{shown}");
    }
}
