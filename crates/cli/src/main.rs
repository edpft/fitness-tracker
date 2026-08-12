//! The operator's entry point, and a composition root.
//!
//! This is the only place besides `web` that names a concrete adapter: it
//! picks the implementations, injects them into the use cases, and translates
//! between the terminal and the driving ports.

mod config;
mod output;

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use application::{
    ExtractWorkouts, Extraction, ExtractionError, ExtractionPorts, ExtractionStatus,
    ReportExtractionStatus, ResetResumptionPoint, ResumptionPointStore, SourceError, StatusError,
    StoreError,
};
use clap::{Arg, ArgMatches, Command as ClapCommand, value_parser};
use domain::landing::{Endpoint, EntityKind, FetchedAt, LandingStream, SourceName};
use infrastructure::{
    EVENTS_ENDPOINT, FileRunLock, HevyWorkoutEvents, HevyWorkoutLandingStore,
    SqliteExtractionRunLog, SqliteResumptionPointStore, connect,
};

use config::{API_KEY_VARIABLE, Config, ConfigError, DEFAULT_BASE_URL};

/// Exit codes are part of the contract: an external scheduler distinguishes
/// outcomes by them, so they are named rather than assorted numbers.
mod exit {
    pub const SUCCESS: u8 = 0;
    /// The source was unreachable or rejected our credential. Raw is
    /// unchanged and the resumption point has not moved.
    pub const SOURCE: u8 = 1;
    /// Another run is already in progress.
    pub const ALREADY_RUNNING: u8 = 2;
    /// The store was unavailable or holds something unreadable.
    pub const STORE: u8 = 3;
    /// Missing configuration, or an unusable invocation.
    pub const USAGE: u8 = 4;
}

/// The command surface, built rather than derived.
///
/// clap's derive macros expand with `#[allow(clippy::restriction)]`, and an
/// `allow` for a forbidden lint is a compile error (E0453) — the panic lints
/// this project forbids all live in that group. The builder API is plain
/// function calls and carries no such attribute, so this is the only way to
/// use clap here at all.
fn command() -> ClapCommand {
    ClapCommand::new("fitness")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Ingest and analyse personal health and fitness data")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .arg(
            Arg::new("database")
                .long("database")
                .env("FITNESS_TRACKER_DATABASE")
                .global(true)
                .value_parser(value_parser!(PathBuf))
                .help("Where the store lives. No path is compiled in"),
        )
        .arg(
            Arg::new("base-url")
                .long("base-url")
                .env("HEVY_API_BASE_URL")
                .global(true)
                .default_value(DEFAULT_BASE_URL)
                .help("The source's base URL"),
        )
        .subcommand(
            ClapCommand::new("extract")
                .about("Collect everything the source has served since the resumption point")
                .arg(source_argument().required(true)),
        )
        .subcommand(
            ClapCommand::new("status")
                .about("Report the most recent successful extraction")
                .arg(source_argument().default_value("hevy")),
        )
        .subcommand(
            ClapCommand::new("reset")
                .about(
                    "Discard the resumption point so the next run collects the full history. \
                     Lands nothing and removes nothing",
                )
                .arg(source_argument().required(true)),
        )
}

fn source_argument() -> Arg {
    Arg::new("source")
        .value_parser(["hevy"])
        .help("Which source to work with")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Hevy,
}

impl Source {
    fn parse(value: Option<&String>) -> Result<Self, Failure> {
        match value.map(String::as_str) {
            Some("hevy") | None => Ok(Self::Hevy),
            Some(other) => Err(Failure {
                message: format!("unknown source {other:?}; the only source is \"hevy\""),
                code: exit::USAGE,
            }),
        }
    }

    fn stream(self) -> Result<LandingStream, Failure> {
        let (source, entity) = match self {
            Self::Hevy => ("hevy", "workouts"),
        };
        Ok(LandingStream::new(
            SourceName::new(source).map_err(|error| Failure::usage(&error))?,
            EntityKind::new(entity).map_err(|error| Failure::usage(&error))?,
        ))
    }
}

/// A message for the operator and a code for whatever invoked us.
struct Failure {
    message: String,
    code: u8,
}

impl Failure {
    fn usage(error: &dyn std::fmt::Display) -> Self {
        Self {
            message: error.to_string(),
            code: exit::USAGE,
        }
    }
}

impl From<ConfigError> for Failure {
    fn from(error: ConfigError) -> Self {
        Self {
            message: error.to_string(),
            code: exit::USAGE,
        }
    }
}

impl From<StoreError> for Failure {
    fn from(error: StoreError) -> Self {
        Self {
            message: error.to_string(),
            code: exit::STORE,
        }
    }
}

impl From<SourceError> for Failure {
    fn from(error: SourceError) -> Self {
        Self {
            message: error.to_string(),
            code: exit::SOURCE,
        }
    }
}

impl From<StatusError> for Failure {
    fn from(error: StatusError) -> Self {
        let StatusError::Store(error) = error;
        Self::from(error)
    }
}

impl From<ExtractionError> for Failure {
    fn from(error: ExtractionError) -> Self {
        let code = match &error {
            // Raw is unchanged and the resumption point has not moved, so this
            // is a degraded system rather than a broken one.
            ExtractionError::Source(_) | ExtractionError::MissingProvenance => exit::SOURCE,
            ExtractionError::AlreadyRunning => exit::ALREADY_RUNNING,
            ExtractionError::Store(_) => exit::STORE,
        };
        Self {
            message: error.to_string(),
            code,
        }
    }
}

fn main() -> ExitCode {
    // `clippy::exit` is `forbid`, so returning `ExitCode` is not a style
    // preference: `std::process::exit` will not compile, and no attribute can
    // grant an exception (E0453).
    let matches = match command().try_get_matches() {
        Ok(matches) => matches,
        Err(error) => {
            // clap knows whether it is printing help, a version, or a usage
            // error, and which stream each belongs on.
            let _ = error.print();
            return if error.use_stderr() {
                ExitCode::from(exit::USAGE)
            } else {
                ExitCode::from(exit::SUCCESS)
            };
        }
    };

    match run(&matches) {
        Ok(()) => ExitCode::from(exit::SUCCESS),
        Err(failure) => {
            eprintln!("fitness: {}", failure.message);
            ExitCode::from(failure.code)
        }
    }
}

fn run(matches: &ArgMatches) -> Result<(), Failure> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| Failure::usage(&error))?;

    runtime.block_on(dispatch(matches))
}

async fn dispatch(matches: &ArgMatches) -> Result<(), Failure> {
    let database = matches.get_one::<PathBuf>("database").cloned();
    let base_url = matches
        .get_one::<String>("base-url")
        .cloned()
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_owned());

    let Some((name, sub)) = matches.subcommand() else {
        return Err(Failure {
            message: "no command given".to_owned(),
            code: exit::USAGE,
        });
    };
    let source = Source::parse(sub.get_one::<String>("source"))?;

    match name {
        "extract" => {
            let config = configure(database, base_url)?;
            extract(&config, source).await
        }
        // Status and reset must work without a credential: a staleness report
        // that needs the source to be reachable reports nothing worth having.
        "status" => status(&database.ok_or(ConfigError::MissingDatabase)?, source).await,
        "reset" => reset(&database.ok_or(ConfigError::MissingDatabase)?, source).await,
        other => Err(Failure {
            message: format!("unknown command {other:?}"),
            code: exit::USAGE,
        }),
    }
}

fn configure(database: Option<PathBuf>, base_url: String) -> Result<Config, Failure> {
    Ok(Config::resolve(
        database,
        base_url,
        std::env::var(API_KEY_VARIABLE),
    )?)
}

async fn extract(config: &Config, source: Source) -> Result<(), Failure> {
    let stream = source.stream()?;
    let pool = connect(&config.database).await?;

    let extraction = Extraction::new(
        stream.clone(),
        Endpoint::new(EVENTS_ENDPOINT).map_err(|error| Failure::usage(&error))?,
        ExtractionPorts {
            source: HevyWorkoutEvents::new(&config.base_url, &config.api_key)?,
            landing: HevyWorkoutLandingStore::new(pool.clone()),
            resumption: SqliteResumptionPointStore::new(pool.clone()),
            runs: SqliteExtractionRunLog::new(pool),
            lock: FileRunLock::beside(&config.database),
            clock: SystemClock,
        },
    );

    output::run_started(&stream);
    let summary = extraction.extract(&stream).await?;
    output::run_succeeded(&summary);
    Ok(())
}

async fn status(database: &Path, source: Source) -> Result<(), Failure> {
    let stream = source.stream()?;
    let pool = connect(database).await?;

    let reader = ExtractionStatus::new(
        HevyWorkoutLandingStore::new(pool.clone()),
        SqliteResumptionPointStore::new(pool.clone()),
        SqliteExtractionRunLog::new(pool),
    );

    output::status(&reader.status(&stream).await?);
    Ok(())
}

async fn reset(database: &Path, source: Source) -> Result<(), Failure> {
    let stream = source.stream()?;
    let pool = connect(database).await?;

    let points = SqliteResumptionPointStore::new(pool.clone());
    let previous = points.read(&stream).await?;

    let reader = ExtractionStatus::new(
        HevyWorkoutLandingStore::new(pool.clone()),
        points,
        SqliteExtractionRunLog::new(pool),
    );
    reader.reset(&stream).await?;

    output::reset(&stream, previous);
    Ok(())
}

/// The wall clock, which is the only thing a real run should take its timings
/// from — and emphatically not where the resumption point comes from.
struct SystemClock;

impl application::Clock for SystemClock {
    fn now(&self) -> FetchedAt {
        FetchedAt::new(jiff::Timestamp::now())
    }
}
