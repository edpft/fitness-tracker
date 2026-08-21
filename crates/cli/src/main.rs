//! The operator's entry point, and a composition root.
//!
//! Translates between the terminal and the driving ports: it reads an
//! invocation, resolves configuration, hands the work to [`wiring`] — which
//! picks the adapters — and turns whatever comes back into output and an exit
//! code.

mod catalogue;
mod config;
mod output;
mod prescribing;
mod wiring;

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use application::{ExtractionError, NormalisationError, SourceError, StatusError, StoreError};
use clap::{Arg, ArgMatches, Command as ClapCommand, value_parser};
use domain::landing::LandingStream;

use catalogue::KnownStream;
use config::{ConfigError, SourceAccess};
use wiring::{Command, Outcome, WiringError};

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
    /// The exercise vocabulary has a gap. A defect in this build rather than in
    /// the data, and its own code because it is the one thing an operator
    /// cannot fix by correcting a record.
    pub const UNMAPPED: u8 = 5;
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
        .subcommand(
            ClapCommand::new("extract")
                .about("Collect everything the source has served since the resumption point")
                .arg(stream_argument())
                // Not global, and not on the other two: only extraction
                // contacts a source, and which source it contacts is decided
                // by the stream this invocation names. The environment
                // variable it overrides is named after that source, so it
                // cannot be declared here — see `catalogue`.
                .arg(Arg::new("base-url").long("base-url").help(
                    "Override the source's API root for this run. \
                             Defaults to <SOURCE>_API_BASE_URL, then to the built-in root",
                )),
        )
        .subcommand(
            ClapCommand::new("normalise")
                .about(
                    "Derive the normalised layer from what raw already holds. \
                     Contacts no source",
                )
                .arg(stream_argument())
                // A flag as well as a variable, because every other input to
                // this command has one and taking half the configuration each
                // way is a worse thing to remember than either. What it must
                // not have is a default: nothing is compiled in, so an
                // invocation that declares no zone is refused rather than
                // guessed at.
                .arg(
                    Arg::new("timezone")
                        .long("timezone")
                        .env("FITNESS_TRACKER_TIMEZONE")
                        .help(
                            "The IANA time zone trained in, such as Europe/London. \
                             Required: no zone is compiled in",
                        ),
                ),
        )
        .subcommand(
            ClapCommand::new("refusals")
                .about("List what the last derivation would not accept, grouped by what to do about it")
                .arg(stream_argument()),
        )
        .subcommand(
            ClapCommand::new("status")
                .about("Report the most recent successful extraction, and the programme in force")
                .arg(stream_argument())
                // Optional here, unlike everywhere else it appears. § 34 forbids a
                // default and § 38 wants staleness observable even when things are
                // wrong — so a status with no zone reports the streams and says
                // what it could not read, rather than refusing to report at all.
                .arg(timezone_argument().required(false)),
        )
        .subcommand(prescribe_command())
        .subcommand(programme_command())
        .subcommand(
            ClapCommand::new("reset")
                .about(
                    "Discard the resumption point so the next run collects the full history. \
                     Lands nothing and removes nothing",
                )
                .arg(stream_argument()),
        )
}

/// Issue a prescription.
///
/// No stream argument: the catalogue is one entry per thing this build can
/// *collect*, and generation collects nothing. A stream here would be a category
/// error, and none of the `SOURCE_`-style environment derivation applies.
fn prescribe_command() -> ClapCommand {
    ClapCommand::new("prescribe")
        .about("Issue the prescription for a date, or show what was already issued")
        .arg(timezone_argument())
        .arg(Arg::new("date").long("date").value_name("date").help(
            "The session to prescribe for, as YYYY-MM-DD. \
                     Defaults to the next programmed day at or after today",
        ))
        .arg(
            Arg::new("reissue")
                .long("reissue")
                .action(clap::ArgAction::SetTrue)
                .help(
                    "Derive the session again and supersede what was already \
                     issued for the date. The superseded prescription is kept, \
                     and stops being the one in force",
                ),
        )
}

/// Author the programme.
fn programme_command() -> ClapCommand {
    ClapCommand::new("programme")
        .about("Author the programme, or report the one in force")
        // Global across `author` and `show`, so `--timezone` may be typed on
        // either side of the subcommand. Both need it and neither has a default.
        .arg(timezone_argument().global(true))
        .subcommand_required(true)
        .subcommand(
            ClapCommand::new("author")
                .about("Read a programme document and store it, superseding the previous one")
                .arg(
                    Arg::new("path")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("The document to read"),
                ),
        )
        .subcommand(ClapCommand::new("show").about(
            "Report the programme in force, its ladder week by week, and which \
                 rung the record puts you on",
        ))
}

/// The zone the operator trains in.
///
/// No default is compiled in, here as everywhere: the zone decides which
/// calendar day a session falls on, and guessing one is an assumption about
/// where the operator lives.
fn timezone_argument() -> Arg {
    Arg::new("timezone")
        .long("timezone")
        .env("FITNESS_TRACKER_TIMEZONE")
        .help(
            "The IANA time zone trained in, such as Europe/London. \
             Required: no zone is compiled in",
        )
}

/// The stream, named the way it is printed back: `source.entity`.
///
/// Source and entity are one argument because they are one name. A flag per
/// half would let `--source hevy --entity routines` be typed for a stream that
/// does not exist, and would still need checking against the catalogue.
fn stream_argument() -> Arg {
    Arg::new("stream")
        .required(true)
        .help("Which stream to work with, as source.entity — for example hevy.workouts")
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

    fn message(message: impl Into<String>, code: u8) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }

    /// What went wrong, for a caller reporting it in place rather than exiting on
    /// it. `status` needs this: an unauthored programme is a state to describe,
    /// not a reason to fail a staleness report.
    fn message_text(&self) -> &str {
        &self.message
    }
}

impl From<ConfigError> for Failure {
    fn from(error: ConfigError) -> Self {
        Self::usage(&error)
    }
}

impl From<StoreError> for Failure {
    fn from(error: StoreError) -> Self {
        Self::message(error.to_string(), exit::STORE)
    }
}

impl From<SourceError> for Failure {
    fn from(error: SourceError) -> Self {
        Self::message(error.to_string(), exit::SOURCE)
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
        Self::message(error.to_string(), code)
    }
}

impl From<NormalisationError> for Failure {
    fn from(error: NormalisationError) -> Self {
        let code = match &error {
            NormalisationError::Store(_) => exit::STORE,
            // Not a data problem, and not something a retry helps with: the
            // mapping is code, so this is a gap to go and fill.
            NormalisationError::UnmappedExercise { .. } => exit::UNMAPPED,
            NormalisationError::MissingTimeZone => exit::USAGE,
        };
        Self::message(error.to_string(), code)
    }
}

impl From<WiringError> for Failure {
    fn from(error: WiringError) -> Self {
        match error {
            WiringError::Extraction(error) => Self::from(error),
            WiringError::Normalisation(error) => Self::from(error),
            WiringError::Status(error) => Self::from(error),
            WiringError::Store(error) => Self::from(error),
            // Both of these are mistakes in this build rather than in the
            // invocation — a stream named but not wired, or an adapter
            // declaring a stream name that is not one — so neither reads as a
            // usage error the operator could act on.
            WiringError::Unwired { .. } | WiringError::Stream(_) => {
                Self::message(error.to_string(), exit::STORE)
            }
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
    let Some((name, sub)) = matches.subcommand() else {
        return Err(Failure::message("no command given", exit::USAGE));
    };

    // Prescription is not a stream. The catalogue is one entry per thing this
    // build can *collect*, and generation collects nothing — so these two are
    // dispatched before any stream is looked up.
    let database = config::database(matches.get_one::<PathBuf>("database").cloned())?;
    match name {
        "prescribe" => {
            let zone = config::timezone(sub.get_one::<String>("timezone").map(String::as_str))?;
            let date = sub.get_one::<String>("date").map(String::as_str);
            let reissue = if sub.get_flag("reissue") {
                application::Reissue::Yes
            } else {
                application::Reissue::No
            };
            return prescribing::prescribe(&database, &zone, date, reissue).await;
        }
        "programme" => {
            let zone = config::timezone(sub.get_one::<String>("timezone").map(String::as_str))?;
            return match sub.subcommand() {
                Some(("author", author)) => {
                    let Some(path) = author.get_one::<PathBuf>("path") else {
                        return Err(Failure::message("no document given", exit::USAGE));
                    };
                    prescribing::author(&database, &zone, path).await
                }
                Some(("show", _)) => prescribing::standing(&database, &zone).await,
                _ => Err(Failure::message("no programme command given", exit::USAGE)),
            };
        }
        _ => {}
    }

    let known = named_stream(sub)?;
    let stream = known
        .landing_stream()
        .map_err(|error| Failure::usage(&error))?;
    let command = match name {
        "extract" => {
            // Printed before the run begins, so a long first collection says
            // what it is doing. The completion line carries the run number,
            // which only the store can assign.
            output::run_started(&stream);
            Command::Extract(source_access(known, sub)?)
        }
        "normalise" => {
            let zone = config::timezone(sub.get_one::<String>("timezone").map(String::as_str))?;
            // Printed before the run begins, so a long first derivation says
            // what it is doing.
            output::derivation_started(&stream);
            Command::Normalise(zone)
        }
        "refusals" => Command::Refusals,
        "status" => Command::Status,
        "reset" => Command::Reset,
        other => {
            return Err(Failure::message(
                format!("unknown command {other:?}"),
                exit::USAGE,
            ));
        }
    };

    let reporting = matches!(command, Command::Status);
    report(&stream, wiring::run(command, known, &database).await?);

    // § 38 on the prescribed side: which programme is in force, where its ladder
    // stands, and how current the record it derives from is. Appended to `status`
    // rather than given a command of its own, because "is anything stale?" is one
    // question and answering half of it is how a stale programme goes unnoticed.
    if reporting {
        prescription_status(
            &database,
            sub.get_one::<String>("timezone").map(String::as_str),
        )
        .await?;
    }
    Ok(())
}

/// The prescription section of `status`, and why it may be absent.
async fn prescription_status(database: &Path, declared: Option<&str>) -> Result<(), Failure> {
    // A blank line, so the prescribed side reads as its own section rather than as
    // more of the derivation's.
    println!();
    let Ok(zone) = config::timezone(declared) else {
        // Not a failure: the stream half of the report is what an operator reaches
        // for when ingestion looks broken, and it must not stop working because a
        // zone is unset. Saying so beats printing nothing.
        println!(
            "prescription — needs a time zone; pass --timezone or set FITNESS_TRACKER_TIMEZONE"
        );
        return Ok(());
    };
    match prescribing::standing(database, &zone).await {
        Ok(()) => Ok(()),
        // An unauthored programme is a legitimate state for a store that only
        // extracts, so it is reported rather than made an exit code.
        Err(failure) => {
            println!("prescription — {}", failure.message_text());
            Ok(())
        }
    }
}

/// The catalogue entry this invocation names.
///
/// Checked against the catalogue rather than against a list clap holds, so the
/// message says what this build can actually do — the two cannot drift apart
/// because there is only one list.
fn named_stream(sub: &ArgMatches) -> Result<&'static KnownStream, Failure> {
    let name = sub
        .get_one::<String>("stream")
        .ok_or_else(|| Failure::message("no stream given", exit::USAGE))?;

    catalogue::lookup(name).ok_or_else(|| {
        Failure::message(
            format!(
                "unknown stream {name:?}; this build collects {}",
                catalogue::known_names()
            ),
            exit::USAGE,
        )
    })
}

/// The base URL comes from the flag, then the source's own variable, then the
/// built-in root; the credential comes from the environment and nowhere else.
fn source_access(known: &KnownStream, sub: &ArgMatches) -> Result<SourceAccess, Failure> {
    let base_url = sub
        .get_one::<String>("base-url")
        .cloned()
        .or_else(|| std::env::var(known.base_url_variable()).ok())
        .unwrap_or_else(|| known.default_base_url().to_owned());

    Ok(SourceAccess::resolve(
        known,
        base_url,
        std::env::var(known.api_key_variable()),
    )?)
}

fn report(stream: &LandingStream, outcome: Outcome) {
    match outcome {
        Outcome::Extracted(summary) => output::run_succeeded(&summary),
        Outcome::Derived(summary) => output::derivation_succeeded(&summary),
        Outcome::Refused(report) => output::refusals(stream, &report),
        Outcome::Reported {
            extraction,
            derivation,
        } => output::status(&extraction, &derivation),
        Outcome::Reset { previous } => output::reset(stream, previous),
    }
}
