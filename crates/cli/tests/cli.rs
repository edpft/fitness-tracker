//! The command surface, exercised as an operator or a scheduler would.
//!
//! Exit codes are the contract here: something invoking `fitness` on a
//! schedule distinguishes outcomes by them and nothing else, so each is
//! pinned. `CARGO_BIN_EXE_fitness` is the binary cargo just built.

use std::{
    fs::OpenOptions,
    path::Path,
    process::{Command, Output},
};

use tempfile::TempDir;

const BINARY: &str = env!("CARGO_BIN_EXE_fitness");

/// Run the binary with a clean environment, so a real `HEVY_API_KEY` or
/// `FITNESS_TRACKER_DATABASE` in the developer's shell cannot change what a
/// test sees.
fn fitness(
    arguments: &[&str],
    database: Option<&Path>,
    api_key: Option<&str>,
) -> std::io::Result<Output> {
    let mut command = Command::new(BINARY);
    command
        .env_remove("HEVY_API_KEY")
        .env_remove("FITNESS_TRACKER_DATABASE")
        .env_remove("FITNESS_TRACKER_TIMEZONE")
        .env_remove("HEVY_API_BASE_URL")
        .args(arguments);

    if let Some(path) = database {
        command.arg("--database").arg(path);
    }
    if let Some(key) = api_key {
        command.env("HEVY_API_KEY", key);
    }

    command.output()
}

fn code(output: &Output) -> i32 {
    output.status.code().unwrap_or(-1)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

// --- Configuration ----------------------------------------------------------

#[test]
fn a_missing_database_path_is_a_usage_error() {
    let output = fitness(&["status", "hevy.workouts"], None, None).expect("the binary runs");
    assert_eq!(code(&output), 4);
    assert!(
        stderr(&output).contains("--database"),
        "{}",
        stderr(&output)
    );
}

/// The credential is env-only and the message says where to get one. It must
/// never be accepted as a flag: a secret on the command line lands in shell
/// history and in `ps` output.
#[test]
fn a_missing_credential_is_a_usage_error_that_says_where_to_get_one() {
    let directory = TempDir::new().expect("a temporary directory");
    let output = fitness(
        &["extract", "hevy.workouts"],
        Some(&directory.path().join("fitness.db")),
        None,
    )
    .expect("the binary runs");

    assert_eq!(code(&output), 4);
    let message = stderr(&output);
    assert!(message.contains("HEVY_API_KEY"), "{message}");
    assert!(message.contains("hevy.com/settings"), "{message}");
    assert!(message.contains("never on the command line"), "{message}");
}

#[test]
fn there_is_no_flag_for_the_credential() {
    let directory = TempDir::new().expect("a temporary directory");
    let output = Command::new(BINARY)
        .env_remove("HEVY_API_KEY")
        .args(["extract", "hevy.workouts", "--api-key", "secret"])
        .arg("--database")
        .arg(directory.path().join("fitness.db"))
        .output()
        .expect("the binary runs");

    assert_eq!(code(&output), 4, "an --api-key flag must not exist");
}

#[test]
fn an_unknown_stream_is_refused_and_says_what_this_build_collects() {
    let directory = TempDir::new().expect("a temporary directory");
    let output = fitness(
        &["extract", "strava.rides"],
        Some(&directory.path().join("fitness.db")),
        Some("a-key"),
    )
    .expect("the binary runs");

    assert_eq!(code(&output), 4);
    let message = stderr(&output);
    assert!(message.contains("strava.rides"), "{message}");
    assert!(message.contains("hevy.workouts"), "{message}");
}

/// A source is not a stream. Naming one without an entity is refused rather
/// than guessed at, because a source that serves two kinds of thing has two
/// resumption points and neither is the default.
#[test]
fn a_source_without_an_entity_is_refused() {
    let directory = TempDir::new().expect("a temporary directory");
    let output = fitness(
        &["status", "hevy"],
        Some(&directory.path().join("fitness.db")),
        None,
    )
    .expect("the binary runs");

    assert_eq!(code(&output), 4);
    assert!(
        stderr(&output).contains("hevy.workouts"),
        "{}",
        stderr(&output)
    );
}

/// The stream is required. Defaulting it would make `status` report on
/// whichever stream happened to be first in the catalogue.
#[test]
fn a_command_without_a_stream_is_a_usage_error() {
    let directory = TempDir::new().expect("a temporary directory");
    let output = fitness(
        &["status"],
        Some(&directory.path().join("fitness.db")),
        None,
    )
    .expect("the binary runs");

    assert_eq!(code(&output), 4);
}

// --- Status -----------------------------------------------------------------

/// Never having run is a fact to report, not an error. And it needs no
/// credential: a staleness report that requires the source to be reachable
/// reports nothing worth having.
#[test]
fn status_before_any_run_succeeds_and_says_never() {
    let directory = TempDir::new().expect("a temporary directory");
    let output = fitness(
        &["status", "hevy.workouts"],
        Some(&directory.path().join("fitness.db")),
        None,
    )
    .expect("the binary runs");

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let report = stdout(&output);
    assert!(report.contains("never"), "{report}");
    assert!(report.contains("hevy.workouts"), "{report}");
    assert!(
        report.contains("unset"),
        "an unset resumption point should say so: {report}"
    );
}

// --- Extraction failures ----------------------------------------------------

/// § 36: a source being unreachable degrades the system rather than failing
/// it. Exit 1, raw unchanged, and other commands keep working.
#[test]
fn an_unreachable_source_exits_one_and_leaves_the_store_usable() {
    let directory = TempDir::new().expect("a temporary directory");
    let database = directory.path().join("fitness.db");

    let output = Command::new(BINARY)
        .env_remove("FITNESS_TRACKER_DATABASE")
        .env("HEVY_API_KEY", "00000000-0000-0000-0000-000000000000")
        // Nothing is listening here, so the connection is refused at once.
        .env("HEVY_API_BASE_URL", "http://127.0.0.1:1")
        .args(["extract", "hevy.workouts"])
        .arg("--database")
        .arg(&database)
        .output()
        .expect("the binary runs");

    assert_eq!(code(&output), 1, "stderr: {}", stderr(&output));

    // The capability that does not depend on the source still answers.
    let after =
        fitness(&["status", "hevy.workouts"], Some(&database), None).expect("the binary runs");
    assert_eq!(code(&after), 0);
    assert!(stdout(&after).contains("never"));
}

/// FR-010. The lock is an ordinary advisory file lock, so a test can hold it
/// without spawning a second copy of the program.
#[test]
fn a_second_run_exits_two_while_the_lock_is_held() {
    let directory = TempDir::new().expect("a temporary directory");
    let database = directory.path().join("fitness.db");

    // Create the store first, so the run under test fails on the lock rather
    // than on anything else.
    assert_eq!(
        code(
            &fitness(&["status", "hevy.workouts"], Some(&database), None).expect("the binary runs")
        ),
        0
    );

    let lock_path = directory.path().join(".hevy.workouts.lock");
    let held = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .expect("a lock file");
    held.try_lock().expect("the test takes the lock first");

    let output = Command::new(BINARY)
        .env_remove("FITNESS_TRACKER_DATABASE")
        .env("HEVY_API_KEY", "00000000-0000-0000-0000-000000000000")
        .env("HEVY_API_BASE_URL", "http://127.0.0.1:1")
        .args(["extract", "hevy.workouts"])
        .arg("--database")
        .arg(&database)
        .output()
        .expect("the binary runs");

    assert_eq!(
        code(&output),
        2,
        "a held lock must exit 2, not attempt the source. stderr: {}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("already in progress"));

    held.unlock().expect("release");
}

// --- Reset ------------------------------------------------------------------

#[test]
fn reset_is_safe_to_run_when_there_is_nothing_to_reset() {
    let directory = TempDir::new().expect("a temporary directory");
    let output = fitness(
        &["reset", "hevy.workouts"],
        Some(&directory.path().join("fitness.db")),
        None,
    )
    .expect("the binary runs");

    assert_eq!(code(&output), 0, "stderr: {}", stderr(&output));
    let message = stdout(&output);
    assert!(message.contains("already unset"), "{message}");
    assert!(
        message.contains("nothing was landed and nothing was removed"),
        "{message}"
    );
}

// --- Help and version -------------------------------------------------------

/// Help is a success, not a usage error — a scheduler that runs `--help` by
/// mistake should not be told the invocation failed.
#[test]
fn help_and_version_exit_zero() {
    for arguments in [["--help"], ["--version"]] {
        let output = Command::new(BINARY)
            .args(arguments)
            .output()
            .expect("the binary runs");
        assert_eq!(code(&output), 0, "{arguments:?}");
    }
}

/// No arguments prints help and reports a usage error, which is what an
/// operator who typed the command wrong needs to see.
#[test]
fn no_arguments_is_a_usage_error() {
    let output = Command::new(BINARY).output().expect("the binary runs");
    assert_eq!(code(&output), 4);
}

// --- Normalisation ----------------------------------------------------------

/// The same clean environment, plus a declared zone.
fn fitness_in(arguments: &[&str], database: Option<&Path>, zone: &str) -> std::io::Result<Output> {
    let mut command = Command::new(BINARY);
    command
        .env_remove("HEVY_API_KEY")
        .env_remove("FITNESS_TRACKER_DATABASE")
        .env_remove("HEVY_API_BASE_URL")
        .env("FITNESS_TRACKER_TIMEZONE", zone)
        .args(arguments);

    if let Some(path) = database {
        command.arg("--database").arg(path);
    }

    command.output()
}

/// § 34 and D4: nothing is compiled in, so a derivation with no declared zone
/// refuses rather than guessing.
///
/// A default of `Europe/London` would be right for this account and wrong for
/// the next, and because it would be right here no test would catch it.
#[test]
fn normalising_without_a_declared_zone_is_a_usage_error() {
    let Ok(directory) = TempDir::new() else {
        panic!("a temporary directory is available")
    };
    let database = directory.path().join("test.db");

    let Ok(output) = fitness(&["normalise", "hevy.workouts"], Some(&database), None) else {
        panic!("the binary runs")
    };

    assert_eq!(code(&output), 4, "usage: {}", stderr(&output));
    let message = stderr(&output);
    assert!(
        message.contains("FITNESS_TRACKER_TIMEZONE"),
        "the message names the variable: {message}"
    );
    assert!(
        message.contains("Europe/London"),
        "and gives an example: {message}"
    );
}

/// A zone that is not an IANA identifier is refused, and the message says so.
/// An offset would be accepted by a laxer parser and is exactly what § II.3
/// rules out.
#[test]
fn a_zone_that_is_not_an_iana_identifier_is_refused() {
    let Ok(directory) = TempDir::new() else {
        panic!("a temporary directory is available")
    };
    let database = directory.path().join("test.db");

    let Ok(output) = fitness_in(&["normalise", "hevy.workouts"], Some(&database), "+01:00") else {
        panic!("the binary runs")
    };

    assert_eq!(code(&output), 4, "usage: {}", stderr(&output));
    assert!(
        stderr(&output).contains("IANA"),
        "the message says what a zone is: {}",
        stderr(&output)
    );
}

/// Deriving over an empty store is not a failure. It is a system with nothing
/// landed yet, which reads differently from one that broke (§ 38).
#[test]
fn normalising_an_empty_store_succeeds_with_nothing_to_do() {
    let Ok(directory) = TempDir::new() else {
        panic!("a temporary directory is available")
    };
    let database = directory.path().join("test.db");

    let Ok(output) = fitness_in(
        &["normalise", "hevy.workouts"],
        Some(&database),
        "Europe/London",
    ) else {
        panic!("the binary runs")
    };

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let printed = stdout(&output);
    assert!(printed.contains("records read"), "{printed}");
    assert!(printed.contains("workouts written"), "{printed}");
}

/// Refusals need no zone: nothing is consulted to produce the list, so
/// demanding one would be asking for configuration to print a table.
#[test]
fn reading_refusals_needs_no_declared_zone() {
    let Ok(directory) = TempDir::new() else {
        panic!("a temporary directory is available")
    };
    let database = directory.path().join("test.db");

    let Ok(derived) = fitness_in(
        &["normalise", "hevy.workouts"],
        Some(&database),
        "Europe/London",
    ) else {
        panic!("the binary runs")
    };
    assert_eq!(code(&derived), 0, "{}", stderr(&derived));

    let Ok(output) = fitness(&["refusals", "hevy.workouts"], Some(&database), None) else {
        panic!("the binary runs")
    };

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("nothing refused"),
        "an empty store refuses nothing: {}",
        stdout(&output)
    );
}
