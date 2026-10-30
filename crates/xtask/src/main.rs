//! The front door to the gate (#629): each run is recorded as spans in the
//! `telemetry.db` beside the store, stamped with the build it checked.
//!
//! - `cargo xtask gate`: every flake check, the fast ones first and stopping
//!   at the first to fail, as a `gate` span with a `check` span for each, and
//!   the `test` spans under `test`.
//! - `cargo xtask check <name>`: one flake check, with no `gate` above it.
//! - `cargo xtask test [<nextest arguments>…]`: `cargo nextest run`, its
//!   `test` spans with no `check` above them.
//! - `cargo xtask import`: CI's runs, recorded the same way. `/start` runs it,
//!   after a merge, so CI's are taken regularly and before Actions deletes what
//!   they uploaded. The gate does not: a local check waits on nothing remote.
//!
//! Nix defines and builds the checks; nix-fast-build runs them and says how
//! each went, and nextest's JUnit report says how each test went.

mod checks;
mod import;
mod junit;

use std::{
    ffi::OsString,
    io::{BufRead as _, BufReader},
    path::{Path, PathBuf},
    process::{Command, ExitCode, ExitStatus, Stdio},
};

use jiff::Timestamp;
use serde_json::Map;
use telemetry::{Outcome, Timed};

/// The workspace: where the flake is, and where cargo puts nextest's reports.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// Where nextest writes its JUnit report, inside the target directory: see
/// `.config/nextest.toml`.
const REPORT: &str = "junit.xml";

/// The checks that build nothing of the workspace, run before the rest so that
/// a slip in one fails the gate in seconds rather than after the cargo builds
/// (#644). A check missing from here only runs later; one named here that the
/// flake no longer has fails the gate's evaluation, so this cannot go stale
/// quietly.
const FAST: &[&str] = &[
    "format",
    "toml-fmt",
    "nix-format",
    "typos",
    "secrets",
    "workflows",
    "architecture",
    "use-case-isolation",
    "workspace-members",
    "unused-deps",
    "audit-deps",
    "audit-licenses",
];

/// How many checks build at once. The laptop serves the operator's app while
/// it builds, so the cargo builds go one at a time, each on every core; the
/// fast checks are light enough for two.
const FAST_JOBS: u8 = 2;
const SLOW_JOBS: u8 = 1;

/// Which of the flake's checks a run builds.
enum Selection<'a> {
    Only(&'a [&'a str]),
    Except(&'a [&'a str]),
}

impl Selection<'_> {
    /// The `--select` function for nix-fast-build.
    fn function(&self) -> String {
        let names = |names: &[&str]| names.join(" ");
        let quoted = |names: &[&str]| {
            names
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(" ")
        };
        match self {
            Self::Only(only) => format!("checks: {{ inherit (checks) {}; }}", names(only)),
            Self::Except(except) => {
                format!("checks: builtins.removeAttrs checks [ {} ]", quoted(except))
            }
        }
    }
}

const USAGE: &str = "usage: cargo xtask gate
       cargo xtask check <name>
       cargo xtask test [<cargo nextest run arguments>...]
       cargo xtask import";

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments.next();
    match command.as_ref().and_then(|command| command.to_str()) {
        Some("gate") if arguments.next().is_none() => gate(),
        Some("check") => match (arguments.next(), arguments.next()) {
            (Some(name), None) => name
                .to_str()
                .filter(|name| is_check(name))
                .map_or_else(usage, check),
            _ => usage(),
        },
        Some("test") => test(&arguments.collect::<Vec<_>>()),
        Some("import") if arguments.next().is_none() => {
            if imported() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        _ => usage(),
    }
}

fn usage() -> ExitCode {
    eprintln!("{USAGE}");
    ExitCode::from(2)
}

/// A flake attribute name, and nothing that could be read as more Nix.
fn is_check(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn gate() -> ExitCode {
    let started_at = Timestamp::now();
    let (mut checks, mut status) = run_checks(&Selection::Only(FAST), FAST_JOBS);
    if matches!(&status, Ok(passed) if passed.success()) {
        let (slow, slow_status) = run_checks(&Selection::Except(FAST), SLOW_JOBS);
        checks.extend(slow);
        status = slow_status;
    }
    let ended_at = Timestamp::now();
    let gate = Timed {
        name: "gate",
        key: None,
        started_at,
        ended_at,
        outcome: Some(outcome(&status)),
        fields: Map::new(),
        children: checks,
    };
    recorded(&[gate]);
    code(status)
}

/// Import CI's runs, and say how it went.
fn imported() -> bool {
    let Some(database) = writable() else {
        return false;
    };
    match import::import(&database) {
        Ok(import::Imported { runs, passed_over }) => {
            eprintln!("xtask: imported {runs} CI runs");
            if passed_over > 0 {
                eprintln!(
                    "xtask: {passed_over} could not be read, and the next import tries them again"
                );
            }
            true
        }
        Err(error) => {
            eprintln!("xtask: CI runs not imported: {error}");
            false
        }
    }
}

fn check(name: &str) -> ExitCode {
    let (checks, status) = run_checks(&Selection::Only(&[name]), SLOW_JOBS);
    recorded(&checks);
    code(status)
}

fn test(arguments: &[OsString]) -> ExitCode {
    let report = target()
        .join("nextest")
        .join(profile(arguments))
        .join(REPORT);
    // A report from an earlier run is not this one's.
    let _ = std::fs::remove_file(&report);
    let status = Command::new("cargo")
        .args(["nextest", "run"])
        .args(arguments)
        .current_dir(ROOT)
        .status();
    let tests = std::fs::read_to_string(&report)
        .map(|report| junit::tests(&report))
        .unwrap_or_default();
    recorded(&tests);
    code(status)
}

/// Run the checks selected, `jobs` at a time, and time each as it ends. The
/// first to fail stops the rest (#644), and is named.
fn run_checks(selection: &Selection, jobs: u8) -> (Vec<Timed>, std::io::Result<ExitStatus>) {
    let system = match system() {
        Ok(system) => system,
        Err(error) => return (Vec::new(), Err(error)),
    };
    let mut command = Command::new("nix-fast-build");
    command
        .args([
            "--no-nom",
            "--skip-cached",
            "--fail-fast",
            "--stream-json-lines",
            "--max-jobs",
        ])
        .arg(jobs.to_string())
        .arg("--flake")
        .arg(format!(".#checks.{system}"))
        .current_dir(ROOT)
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    command.arg("--select").arg(selection.function());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return (Vec::new(), Err(error)),
    };
    let mut checks = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Some(check) = checks::ended(&line, Timestamp::now(), tests_in) else {
                continue;
            };
            if check.outcome == Some(Outcome::Failed) {
                let name = check.fields.get("check").and_then(|name| name.as_str());
                eprintln!("xtask: stopped: {} failed", name.unwrap_or("a check"));
            }
            checks.push(check);
        }
    }
    (checks, child.wait())
}

/// The tests a built check ran, if it is one that keeps nextest's report.
fn tests_in(out: &Path) -> Vec<Timed> {
    std::fs::read_to_string(out.join(REPORT))
        .map(|report| junit::tests(&report))
        .unwrap_or_default()
}

fn system() -> std::io::Result<String> {
    let output = Command::new("nix")
        .args([
            "eval",
            "--impure",
            "--raw",
            "--expr",
            "builtins.currentSystem",
        ])
        .stderr(Stdio::inherit())
        .output()?;
    String::from_utf8(output.stdout)
        .ok()
        .filter(|system| output.status.success() && !system.is_empty())
        .ok_or_else(|| std::io::Error::other("nix could not name this system"))
}

/// nextest's profile, as `--profile` or `-P` names it.
fn profile(arguments: &[OsString]) -> String {
    arguments
        .iter()
        .zip(arguments.iter().skip(1))
        .find(|(flag, _)| *flag == "--profile" || *flag == "-P")
        .and_then(|(_, profile)| profile.to_str())
        .or_else(|| {
            arguments
                .iter()
                .find_map(|argument| argument.to_str()?.strip_prefix("--profile="))
        })
        .unwrap_or("default")
        .to_owned()
}

fn target() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| Path::new(ROOT).join("target"), PathBuf::from)
}

fn outcome(status: &std::io::Result<ExitStatus>) -> Outcome {
    match status {
        Ok(status) if status.success() => Outcome::Succeeded,
        _ => Outcome::Failed,
    }
}

fn code(status: std::io::Result<ExitStatus>) -> ExitCode {
    match status {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .map_or(ExitCode::FAILURE, ExitCode::from),
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Write `spans`. One that does not land is said, and changes nothing else: the
/// run it records passed or failed on its own account.
fn recorded(spans: &[Timed]) {
    let Some(database) = writable() else {
        return;
    };
    if let Err(error) = telemetry::write(&database, build().as_deref(), spans) {
        eprintln!("xtask: not recorded in {}: {error}", database.display());
    }
}

/// The `telemetry.db`, with its directory made if it was not there.
fn writable() -> Option<PathBuf> {
    let Some(database) = database() else {
        eprintln!("xtask: not recorded: neither XDG_DATA_HOME nor HOME is an absolute path");
        return None;
    };
    if let Some(directory) = database.parent() {
        let _ = std::fs::create_dir_all(directory);
    }
    Some(database)
}

/// The `telemetry.db` beside the store, where the operator's usage is, so that
/// the two join on `build`. The store's own rule: `XDG_DATA_HOME`, else
/// `~/.local/share`, then `fitness-tracker`.
fn database() -> Option<PathBuf> {
    let absolute = |variable: &str| {
        std::env::var_os(variable)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    absolute("XDG_DATA_HOME")
        .or_else(|| absolute("HOME").map(|home| home.join(".local").join("share")))
        .map(|base| base.join("fitness-tracker").join("telemetry.db"))
}

/// The build checked, as `fitness --version` names one: the short commit, with
/// `-dirty` for a tree with changes nix would see, and the commit's date.
fn build() -> Option<String> {
    let git = |arguments: &[&str]| {
        Command::new("git")
            .args(arguments)
            .current_dir(ROOT)
            .env("TZ", "UTC")
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|output| output.trim().to_owned())
    };
    let revision = git(&["rev-parse", "--short=7", "HEAD"])?;
    let dirty = !git(&["status", "--porcelain", "--untracked-files=no"])?.is_empty();
    let date = git(&[
        "show",
        "-s",
        "--format=%cd",
        "--date=format-local:%Y-%m-%d",
        "HEAD",
    ])?;
    let suffix = if dirty { "-dirty" } else { "" };
    Some(format!("{revision}{suffix} ({date})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fast_pass_builds_only_the_fast_checks() {
        assert_eq!(
            Selection::Only(&["format", "typos"]).function(),
            "checks: { inherit (checks) format typos; }"
        );
    }

    #[test]
    fn the_slow_pass_builds_every_check_but_those() {
        assert_eq!(
            Selection::Except(&["format", "typos"]).function(),
            r#"checks: builtins.removeAttrs checks [ "format" "typos" ]"#
        );
    }
}
