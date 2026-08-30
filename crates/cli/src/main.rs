//! The operator's entry point, and a composition root.
//!
//! Translates between the terminal and the driving ports: it reads an
//! invocation, resolves configuration, hands the work to [`wiring`] — which
//! picks the adapters — and turns whatever comes back into output and an exit
//! code.

mod candidates;
mod catalogue;
mod config;
mod output;
mod parameters;
mod paths;
mod prescribing;
mod scheduling;
mod setup;
mod wiring;
mod wizard;

use std::{
    path::{Path, PathBuf},
    process::ExitCode,
};

use application::{ExtractionError, NormalisationError, SourceError, StatusError, StoreError};
use clap::{Arg, ArgAction, ArgMatches, Command as ClapCommand, value_parser};
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
        .subcommand(init_command())
        .subcommand(prescribe_command())
        .subcommand(deliver_command())
        .subcommand(compare_command())
        .subcommand(programme_command())
        .subcommand(parameters_command())
        .subcommand(schedule_command())
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
/// Read a session back against what it was told to be.
fn compare_command() -> ClapCommand {
    ClapCommand::new("compare")
        .about("Report how a performed session differed from the one prescribed for it")
        .arg(timezone_argument())
        .arg(Arg::new("date").long("date").value_name("date").help(
            "The session to compare, as YYYY-MM-DD — the date it was prescribed \
                     for, not the day it was trained. Defaults to the next programmed \
                     day at or after today",
        ))
}

fn prescribe_command() -> ClapCommand {
    ClapCommand::new("prescribe")
        .about("Derive the session for a date against the record as it now stands, and issue it if it differs from what is already in force")
        .arg(timezone_argument())
        .arg(Arg::new("date").long("date").value_name("date").help(
            "The session to prescribe for, as YYYY-MM-DD. \
                     Defaults to the next programmed day at or after today",
        ))
}

/// Add the programme, or report the one in force.
/// Prepare a machine.
///
/// **First in the list because it is first in the order.** An operator who has
/// just installed the binary should find the command that gets them started
/// before the ones that need a store to already exist.
fn init_command() -> ClapCommand {
    ClapCommand::new("init")
        .about("Create the store and the settings file, and report what is still needed")
        .arg(
            Arg::new("timezone")
                .long("timezone")
                .value_name("iana")
                .env("FITNESS_TRACKER_TIMEZONE")
                .help(
                    "The IANA time zone trained in, such as Europe/London. Asked for at a \
                     terminal if it is not given",
                ),
        )
        .arg(
            Arg::new("force")
                .long("force")
                .action(ArgAction::SetTrue)
                .help("Replace an existing settings file. It is hand-edited, so this refuses by default"),
        )
        .arg(
            Arg::new("api-key-stdin")
                .long("api-key-stdin")
                .action(ArgAction::SetTrue)
                .help(
                    "Read the source's API key from standard input, for piping from a \
                     password manager. There is no flag to pass it directly: a secret in \
                     argv lands in shell history and in `ps` output",
                ),
        )
}

/// Put an issued session where the operator trains from.
///
/// A command of its own rather than a flag on `prescribe`, because the two fail
/// for unrelated reasons: § 36 wants a destination being unreachable to leave a
/// perfectly good prescription in the store, and folding them together would
/// make one exit code answer for both.
fn deliver_command() -> ClapCommand {
    ClapCommand::new("deliver")
        .about("Put the session issued for a date where the operator trains from")
        .arg(Arg::new("date").long("date").value_name("date").help(
            "The session to deliver, as YYYY-MM-DD. Defaults to the next \
                     programmed day at or after today",
        ))
        .arg(
            Arg::new("preview")
                .long("preview")
                .action(ArgAction::SetTrue)
                .help(
                    "Render the session and print what would be sent, without \
                     sending it or recording anything",
                ),
        )
        .arg(timezone_argument())
}

fn programme_command() -> ClapCommand {
    ClapCommand::new("programme")
        .about("Add a programme, or report the one in force")
        // Global across `add` and `show`, so `--timezone` may be typed on
        // either side of the subcommand. Both need it and neither has a default.
        .arg(timezone_argument().global(true))
        .subcommand_required(true)
        .subcommand(
            ClapCommand::new("add")
                .about(
                    "Ask what the block is and write it down, or read a document \
                     already written. Either way it supersedes the previous one \
                     of that name",
                )
                .arg(
                    Arg::new("path")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help(
                            "The document to read. Omit it and the questions are \
                             asked instead, and the answers written to a document",
                        ),
                )
                .arg(
                    Arg::new("into")
                        .long("into")
                        .value_parser(clap::value_parser!(PathBuf))
                        .conflicts_with("path")
                        .help(
                            "Where to write the document the questions produce. \
                             Defaults to the block's name in the working directory",
                        ),
                ),
        )
        .subcommand(
            ClapCommand::new("show")
                .about(
                    "Report the programme in force on a date, its plan week by \
                     week, and which rung the record puts you on",
                )
                .arg(Arg::new("date").long("date").value_name("date").help(
                    "The day to report on, as YYYY-MM-DD. Defaults to today — \
                     programmes succeed one another, so which one answers \
                     depends on the date",
                )),
        )
}

/// The numbers a prescription is generated against.
///
/// **Show and nothing else, for now.** They are seeded by `init` and there is
/// no way to change one from here yet; a `change` that only some of them
/// accepted would be worse than an honest gap. What this closes is decision
/// 0015's own guard — a shipped default is never invisible.
fn parameters_command() -> ClapCommand {
    ClapCommand::new("parameters")
        .about("Report the numbers every prescription is generated against")
        .subcommand_required(true)
        .subcommand(
            ClapCommand::new("show").about("Print every parameter in force, and when it was set"),
        )
}

fn schedule_command() -> ClapCommand {
    ClapCommand::new("schedule")
        .about("Record when there is room to train, or report it")
        // No `--timezone`. The zone is *in* the pattern, and in any alteration
        // that moves it — asking for one on the way in would be asking for the
        // answer.
        .subcommand_required(true)
        .subcommand(
            ClapCommand::new("add")
                .about("Ask when you ordinarily have room to train, and record it"),
        )
        .subcommand(
            ClapCommand::new("alter")
                .about("Ask what departs from the ordinary pattern, and record it"),
        )
        .subcommand(
            ClapCommand::new("show")
                .about("Report the ordinary pattern and everything that departs from it"),
        )
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
#[derive(Debug)]
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

/// Where this invocation keeps things.
///
/// Its own function rather than the top of [`dispatch`], because working out
/// where the store and the settings live is a different question from which
/// command was asked for, and only one of the two is about the operator's
/// intent.
struct Locations {
    settings: infrastructure::Settings,
    credentials: infrastructure::Credentials,
    /// Named even where it could not be resolved, so a message can say what to
    /// create.
    settings_path: PathBuf,
    database: PathBuf,
}

/// Resolve them, in the order a value earns precedence: what this invocation
/// passed, then what the operator stated once, then the specification's default.
///
/// # Errors
///
/// [`Failure`] if the settings file exists but will not parse, or if a store
/// path is needed and no base directory can be worked out.
fn locations(matches: &ArgMatches) -> Result<Locations, Failure> {
    // **Settings are read once, and a machine that cannot say where they live
    // simply has none.** Failing here would refuse to run for want of a file
    // that may hold nothing an invocation needs — every value in it can be
    // passed explicitly.
    let environment = paths::SystemEnvironment;
    let resolved = paths::settings(&environment).ok();
    let settings = match resolved.as_deref() {
        Some(path) => infrastructure::Settings::read(path).map_err(config::ConfigError::from)?,
        None => infrastructure::Settings::default(),
    };
    let credentials = match resolved.as_deref() {
        Some(path) => infrastructure::Credentials::read(&infrastructure::credentials::beside(path))
            .map_err(config::ConfigError::from)?,
        None => infrastructure::Credentials::default(),
    };

    // The default store is resolved only if nothing else supplied one, so a
    // machine with no home directory still works when the path is passed.
    let database =
        match config::database(matches.get_one::<PathBuf>("database").cloned(), &settings) {
            Some(path) => path,
            None => paths::store(&environment).map_err(config::ConfigError::from)?,
        };

    // **The store creates itself; its directory does not.** SQLite will make the
    // file but not the path to it, so a first run on a fresh machine would fail
    // with an error about a database rather than about a missing directory.
    paths::ensure_parent(&database).map_err(|error| {
        Failure::message(
            format!(
                "cannot create the directory for {}: {error}",
                database.display()
            ),
            exit::STORE,
        )
    })?;

    Ok(Locations {
        settings,
        credentials,
        settings_path: resolved.unwrap_or_else(|| PathBuf::from("the settings file")),
        database,
    })
}

/// The commands that need no stream.
///
/// **Prescription is not a stream.** The catalogue is one entry per thing this
/// build can *collect*, and neither generating a session nor preparing a machine
/// collects anything — so these are answered before a stream is looked up, and
/// `None` means the name was not one of theirs.
async fn authored_command(
    name: &str,
    sub: &ArgMatches,
    settings: &infrastructure::Settings,
    credentials: &infrastructure::Credentials,
    settings_path: &Path,
    database: &Path,
) -> Option<Result<(), Failure>> {
    // Every one of these needs the zone, and asking for it here would refuse
    // `init` — which is the command whose whole job is to obtain one.
    let zone = |sub: &ArgMatches| {
        config::timezone(
            sub.get_one::<String>("timezone").map(String::as_str),
            settings,
            settings_path,
        )
    };

    match name {
        "init" => {
            let prepared = match setup::init(
                settings_path,
                database,
                sub.get_one::<String>("timezone").map(String::as_str),
                sub.get_flag("force"),
                sub.get_flag("api-key-stdin"),
            )
            .await
            {
                Ok(prepared) => prepared,
                Err(failure) => return Some(Err(failure)),
            };
            output::prepared(&prepared);
            Some(Ok(()))
        }
        "prescribe" => {
            let zone = match zone(sub) {
                Ok(zone) => zone,
                Err(error) => return Some(Err(error.into())),
            };
            Some(
                prescribing::prescribe(
                    database,
                    &zone,
                    sub.get_one::<String>("date").map(String::as_str),
                )
                .await,
            )
        }
        "compare" => {
            let zone = match zone(sub) {
                Ok(zone) => zone,
                Err(error) => return Some(Err(error.into())),
            };
            Some(
                prescribing::compare(
                    database,
                    &zone,
                    sub.get_one::<String>("date").map(String::as_str),
                )
                .await,
            )
        }
        "deliver" => {
            let zone = match zone(sub) {
                Ok(zone) => zone,
                Err(error) => return Some(Err(error.into())),
            };
            Some(
                prescribing::deliver(
                    database,
                    &zone,
                    sub.get_one::<String>("date").map(String::as_str),
                    sub.get_flag("preview"),
                    credentials,
                )
                .await,
            )
        }
        "parameters" => Some(match sub.subcommand() {
            Some(("show", _)) => prescribing::parameters(database).await,
            _ => Err(Failure::message("no parameters command given", exit::USAGE)),
        }),
        "schedule" => Some(match sub.subcommand() {
            Some(("add", _)) => scheduling::add(database).await,
            Some(("alter", _)) => scheduling::alter(database).await,
            Some(("show", _)) => scheduling::show(database).await,
            _ => Err(Failure::message("no schedule command given", exit::USAGE)),
        }),
        "programme" => {
            let zone = match zone(sub) {
                Ok(zone) => zone,
                Err(error) => return Some(Err(error.into())),
            };
            Some(programme_command_run(sub, database, &zone).await)
        }
        _ => None,
    }
}

/// `programme add` and `programme show`, once the zone is in hand.
///
/// Its own function only because the arm outgrew the match it lived in.
async fn programme_command_run(
    sub: &ArgMatches,
    database: &Path,
    zone: &domain::gym::OperatorZone,
) -> Result<(), Failure> {
    match sub.subcommand() {
        Some(("add", add)) => match add.get_one::<PathBuf>("path") {
            Some(path) => prescribing::add(database, zone, path).await,
            // No document: ask, write one, and author that.
            None => {
                wizard::add(
                    database,
                    zone,
                    add.get_one::<PathBuf>("into").map(PathBuf::as_path),
                )
                .await
            }
        },
        Some(("show", show)) => {
            prescribing::standing(
                database,
                zone,
                show.get_one::<String>("date").map(String::as_str),
            )
            .await
        }
        _ => Err(Failure::message("no programme command given", exit::USAGE)),
    }
}

async fn dispatch(matches: &ArgMatches) -> Result<(), Failure> {
    let Some((name, sub)) = matches.subcommand() else {
        return Err(Failure::message("no command given", exit::USAGE));
    };

    let Locations {
        settings,
        credentials,
        settings_path,
        database,
    } = locations(matches)?;

    if let Some(outcome) = authored_command(
        name,
        sub,
        &settings,
        &credentials,
        &settings_path,
        &database,
    )
    .await
    {
        return outcome;
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
            Command::Extract(source_access(known, sub, &credentials)?)
        }
        "normalise" => {
            let zone = config::timezone(
                sub.get_one::<String>("timezone").map(String::as_str),
                &settings,
                &settings_path,
            )?;
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
            &settings,
            &settings_path,
        )
        .await?;
    }
    Ok(())
}

/// The prescription section of `status`, and why it may be absent.
async fn prescription_status(
    database: &Path,
    declared: Option<&str>,
    settings: &infrastructure::Settings,
    settings_path: &Path,
) -> Result<(), Failure> {
    // A blank line, so the prescribed side reads as its own section rather than as
    // more of the derivation's.
    println!();
    let Ok(zone) = config::timezone(declared, settings, settings_path) else {
        // Not a failure: the stream half of the report is what an operator reaches
        // for when ingestion looks broken, and it must not stop working because a
        // zone is unset. Saying so beats printing nothing.
        println!(
            "prescription — needs a time zone; pass --timezone or set FITNESS_TRACKER_TIMEZONE"
        );
        return Ok(());
    };
    match prescribing::standing(database, &zone, None).await {
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
fn source_access(
    known: &KnownStream,
    sub: &ArgMatches,
    credentials: &infrastructure::Credentials,
) -> Result<SourceAccess, Failure> {
    let base_url = sub
        .get_one::<String>("base-url")
        .cloned()
        .or_else(|| std::env::var(known.base_url_variable()).ok())
        .unwrap_or_else(|| known.default_base_url().to_owned());

    Ok(SourceAccess::resolve(
        known.source(),
        base_url,
        std::env::var(known.api_key_variable()),
        credentials.key(known.source().name()),
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
