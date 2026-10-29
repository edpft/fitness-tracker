//! CI's runs, as the spans a local run records: a run of Continuous
//! integration is a `gate`, a job that runs a flake check is a `check` timed
//! as its `Run check` step, its tests come from the JUnit report the job
//! uploaded, and every other job is a `job`.
//!
//! Through `gh`, which holds the operator's login. Each span is keyed on
//! GitHub's ids, so importing a run twice leaves it once, and a run already
//! imported is not fetched again. Actions deletes an artifact after 90 days,
//! so an import that waits longer records that run's checks without their
//! cache status or their tests.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use jiff::Timestamp;
use serde::Deserialize;
use serde_json::{Map, Value};
use telemetry::{Outcome, Timed};

/// The workflow that is the gate, as `.github/workflows/ci.yml` names it.
const GATE: &str = "Continuous integration";

/// Its check jobs' names, until 2026-10-29: they came from a reusable
/// workflow, and GitHub prefixed them with its job's name.
const REUSED: &str = "checks / ";

/// The step a check job runs its check in, and the one it skips it with.
const RUN_CHECK: &str = "Run check";
const SKIP_CHECK: &str = "Skip, unchanged since the base";

/// The nightly audit's step, and the check it runs.
const RUN_AUDIT: &str = "Run audit";
const AUDIT: &str = "audit-deps";

#[derive(Debug, Deserialize)]
pub struct Run {
    id: u64,
    name: String,
    #[serde(rename = "run_attempt")]
    attempt: u64,
    event: String,
    head_branch: Option<String>,
    head_sha: String,
    head_commit: Option<Commit>,
    conclusion: Option<String>,
    #[serde(rename = "run_started_at", deserialize_with = "timestamp")]
    started_at: Timestamp,
    #[serde(deserialize_with = "timestamp")]
    created_at: Timestamp,
    #[serde(deserialize_with = "timestamp")]
    updated_at: Timestamp,
}

#[derive(Debug, Deserialize)]
struct Commit {
    #[serde(deserialize_with = "timestamp")]
    timestamp: Timestamp,
}

#[derive(Debug, Deserialize)]
pub struct Job {
    id: u64,
    name: String,
    conclusion: Option<String>,
    #[serde(deserialize_with = "maybe_timestamp")]
    started_at: Option<Timestamp>,
    #[serde(deserialize_with = "maybe_timestamp")]
    completed_at: Option<Timestamp>,
    #[serde(default)]
    steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
struct Step {
    name: String,
    conclusion: Option<String>,
    #[serde(default, deserialize_with = "maybe_timestamp")]
    started_at: Option<Timestamp>,
    #[serde(default, deserialize_with = "maybe_timestamp")]
    completed_at: Option<Timestamp>,
}

// jiff's `serde` feature would be one more feature unified across the
// workspace, and so a rebuild of every dependency, for three fields.
fn timestamp<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Timestamp, D::Error> {
    String::deserialize(deserializer)?
        .parse()
        .map_err(serde::de::Error::custom)
}

fn maybe_timestamp<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Timestamp>, D::Error> {
    Option::<String>::deserialize(deserializer)?
        .map(|timestamp| timestamp.parse().map_err(serde::de::Error::custom))
        .transpose()
}

/// What a check job uploaded: nix-fast-build's result and nextest's report.
#[derive(Debug, Default)]
pub struct Results {
    pub cache_status: Option<String>,
    pub tests: Vec<Timed>,
}

impl Run {
    fn key(&self) -> String {
        format!("ci/run/{}/{}", self.id, self.attempt)
    }

    /// As `fitness --version` names a build: the short commit and its date.
    pub fn build(&self) -> String {
        let revision = self.head_sha.get(..7).unwrap_or(&self.head_sha);
        self.head_commit.as_ref().map_or_else(
            || revision.to_owned(),
            |commit| format!("{revision} ({})", commit.timestamp.strftime("%Y-%m-%d")),
        )
    }
}

/// The spans one run leaves. `results` reads what a check job uploaded.
pub fn spans(run: &Run, jobs: &[Job], results: impl Fn(&str) -> Results) -> Vec<Timed> {
    let mut fields = Map::new();
    fields.insert("workflow".into(), run.name.clone().into());
    fields.insert("run_id".into(), run.id.into());
    let jobs = jobs.iter().filter_map(|job| span(&fields, job, &results));
    if run.name != GATE {
        return jobs.collect();
    }
    let mut gate = fields.clone();
    gate.insert("attempt".into(), run.attempt.into());
    gate.insert("event".into(), run.event.clone().into());
    if let Some(branch) = &run.head_branch {
        gate.insert("branch".into(), branch.clone().into());
    }
    concluded(&mut gate, run.conclusion.as_deref());
    vec![Timed {
        name: "gate",
        key: Some(run.key()),
        started_at: run.started_at,
        ended_at: run.updated_at,
        outcome: Some(outcome(run.conclusion.as_deref())),
        fields: gate,
        children: jobs.collect(),
    }]
}

fn span(run: &Map<String, Value>, job: &Job, results: impl Fn(&str) -> Results) -> Option<Timed> {
    let mut fields = run.clone();
    fields.insert("job_id".into(), job.id.into());
    let step = |name: &str| job.steps.iter().find(|step| step.name == name);
    let ran =
        |step: &Step| step.started_at.is_some() && step.conclusion.as_deref() != Some("skipped");
    let name = job.name.strip_prefix(REUSED).unwrap_or(&job.name);

    if let Some(skip) = step(SKIP_CHECK).filter(|skip| ran(skip)) {
        fields.insert("check".into(), name.into());
        fields.insert("cache_status".into(), "unchanged".into());
        return timed(
            "check",
            job,
            skip,
            Some(Outcome::Skipped),
            fields,
            Vec::new(),
        );
    }
    let check = step(RUN_CHECK)
        .map(|step| (name, step))
        .or_else(|| step(RUN_AUDIT).map(|step| (AUDIT, step)));
    if let Some((check, step)) = check.filter(|(_, step)| ran(step)) {
        let Results {
            cache_status,
            tests,
        } = results(check);
        fields.insert("check".into(), check.into());
        if let Some(cache_status) = cache_status {
            fields.insert("cache_status".into(), cache_status.into());
        }
        concluded(&mut fields, step.conclusion.as_deref());
        let outcome = outcome(step.conclusion.as_deref());
        return timed("check", job, step, Some(outcome), fields, tests);
    }

    fields.insert("job".into(), name.into());
    concluded(&mut fields, job.conclusion.as_deref());
    let started_at = job.started_at?;
    Some(Timed {
        name: "job",
        key: Some(format!("ci/job/{}", job.id)),
        started_at,
        ended_at: job.completed_at.unwrap_or(started_at),
        outcome: Some(outcome(job.conclusion.as_deref())),
        fields,
        children: Vec::new(),
    })
}

fn timed(
    name: &'static str,
    job: &Job,
    step: &Step,
    outcome: Option<Outcome>,
    fields: Map<String, Value>,
    children: Vec<Timed>,
) -> Option<Timed> {
    let started_at = step.started_at?;
    Some(Timed {
        name,
        key: Some(format!("ci/job/{}", job.id)),
        started_at,
        ended_at: step.completed_at.unwrap_or(started_at),
        outcome,
        fields,
        children,
    })
}

/// GitHub's own word, since `cancelled` and `timed_out` are both failed here.
fn concluded(fields: &mut Map<String, Value>, conclusion: Option<&str>) {
    if let Some(conclusion) = conclusion {
        fields.insert("conclusion".into(), conclusion.into());
    }
}

fn outcome(conclusion: Option<&str>) -> Outcome {
    match conclusion {
        Some("success") => Outcome::Succeeded,
        Some("skipped" | "neutral") => Outcome::Skipped,
        _ => Outcome::Failed,
    }
}

/// Read what the check job for `check` uploaded, from `directory`.
pub fn uploaded(directory: &Path, check: &str) -> Results {
    let here = directory.join(format!("check-{check}"));
    let cache_status = std::fs::read_to_string(here.join("result.json"))
        .ok()
        .and_then(|result| serde_json::from_str::<Value>(&result).ok())
        .and_then(|result| {
            result
                .get("results")?
                .as_array()?
                .iter()
                .find(|line| line.get("type").and_then(Value::as_str) == Some("EVAL"))?
                .get("cacheStatus")?
                .as_str()
                .map(str::to_owned)
        });
    let tests = std::fs::read_to_string(here.join(super::REPORT))
        .map(|report| super::junit::tests(&report))
        .unwrap_or_default();
    Results {
        cache_status,
        tests,
    }
}

/// How many runs an import recorded, and how many it could not read.
pub struct Imported {
    pub runs: usize,
    pub passed_over: usize,
}

/// Import every completed run not already in `database`.
pub fn import(database: &Path) -> Result<Imported, String> {
    let repository = gh(&[
        "repo",
        "view",
        "--json",
        "nameWithOwner",
        "--jq",
        ".nameWithOwner",
    ])?;
    let repository = repository.trim();
    let runs = listed(repository)?;
    let mut pending = Vec::new();
    for run in &runs {
        if !telemetry::is_written(database, &run.key()).map_err(|error| error.to_string())? {
            pending.push(run);
        }
    }
    let mut imported = 0;
    let mut passed_over = 0;
    // `gh` is most of the time an import takes, so several runs are fetched at
    // once, and written one at a time.
    for batch in pending.chunks(AT_ONCE) {
        let fetched: Vec<_> = std::thread::scope(|scope| {
            // Every run is started before any is waited for.
            let mut fetching = Vec::with_capacity(batch.len());
            for run in batch {
                fetching.push(scope.spawn(|| fetched(repository, run)));
            }
            fetching
                .into_iter()
                .map(|fetching| fetching.join().ok().flatten())
                .collect()
        });
        for (run, fetched) in batch.iter().zip(fetched) {
            // A run whose jobs cannot be read now is passed over, and the next
            // import tries it again: GitHub answers 502 now and then.
            let Some((first, spans)) = fetched else {
                passed_over += 1;
                continue;
            };
            if let Some(first) = first
                && run.name != GATE
                && telemetry::is_written(database, &format!("ci/job/{first}"))
                    .map_err(|error| error.to_string())?
            {
                continue;
            }
            telemetry::write(database, Some(&run.build()), &spans)
                .map_err(|error| error.to_string())?;
            imported += 1;
            if imported % 100 == 0 {
                eprintln!("xtask: imported {imported} runs");
            }
        }
    }
    Ok(Imported {
        runs: imported,
        passed_over,
    })
}

/// Every completed run. GitHub lists at most 1,000 for one query, so each
/// query after the first asks for the runs created before the oldest so far.
fn listed(repository: &str) -> Result<Vec<Run>, String> {
    let mut runs: Vec<Run> = Vec::new();
    let mut before: Option<Timestamp> = None;
    loop {
        let created = before.map_or_else(String::new, |before| format!("&created=<{before}"));
        let page: Vec<Run> = lines(&gh(&[
            "api",
            "--paginate",
            &format!("repos/{repository}/actions/runs?per_page=100&status=completed{created}"),
            "--jq",
            ".workflow_runs[]",
        ])?)?;
        let oldest = page.iter().map(|run| run.created_at).min();
        let fewer = oldest.is_none() || oldest == before;
        runs.extend(page);
        if fewer {
            return Ok(runs);
        }
        before = oldest;
    }
}

/// How many runs are fetched at once.
const AT_ONCE: usize = 8;

/// A run's spans, and its first job's id, or nothing if its jobs could not be
/// read.
fn fetched(repository: &str, run: &Run) -> Option<(Option<u64>, Vec<Timed>)> {
    let jobs = gh(&[
        "api",
        "--paginate",
        &format!(
            "repos/{repository}/actions/runs/{}/attempts/{}/jobs?per_page=100",
            run.id, run.attempt
        ),
        "--jq",
        ".jobs[]",
    ])
    .and_then(|jobs| lines::<Job>(&jobs))
    .ok()?;
    let directory = downloaded(repository, run);
    let spans = spans(run, &jobs, |check| {
        directory
            .as_deref()
            .map(|directory| uploaded(directory, check))
            .unwrap_or_default()
    });
    if let Some(directory) = &directory {
        let _ = std::fs::remove_dir_all(directory);
    }
    Some((jobs.first().map(|job| job.id), spans))
}

/// Actions keeps an artifact this long, so an older run has none to ask for.
const KEPT: jiff::SignedDuration = jiff::SignedDuration::from_hours(90 * 24);

/// The run's artifacts, if it uploaded any still kept.
fn downloaded(repository: &str, run: &Run) -> Option<PathBuf> {
    if run.name != GATE || Timestamp::now().duration_since(run.started_at) > KEPT {
        return None;
    }
    let names = gh(&[
        "api",
        &format!(
            "repos/{repository}/actions/runs/{}/artifacts?per_page=100",
            run.id
        ),
        "--jq",
        r#".artifacts[] | select(.expired | not) | .name | select(startswith("check-"))"#,
    ])
    .ok()?;
    if names.trim().is_empty() {
        return None;
    }
    let directory = std::env::temp_dir().join(format!("xtask-import-{}", run.id));
    let _ = std::fs::remove_dir_all(&directory);
    gh(&[
        "run",
        "download",
        &run.id.to_string(),
        "--repo",
        repository,
        "--dir",
        &directory.to_string_lossy(),
    ])
    .ok()?;
    Some(directory)
}

fn gh(arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("gh")
        .args(arguments)
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("gh: {error}"))?;
    if !output.status.success() {
        return Err(format!("gh {} failed", arguments.join(" ")));
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

fn lines<T: for<'de> Deserialize<'de>>(output: &str) -> Result<Vec<T>, String> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|error| error.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(name: &str) -> Run {
        serde_json::from_value(serde_json::json!({
            "id": 40_924_676_727_u64,
            "name": name,
            "run_attempt": 1,
            "event": "push",
            "head_branch": "main",
            "head_sha": "7fc5a449764352328b3065fffb4d166d47dbf422",
            "head_commit": {"timestamp": "2026-10-29T17:47:26Z"},
            "conclusion": "success",
            "run_started_at": "2026-10-29T17:47:29Z",
            "created_at": "2026-10-29T17:47:28Z",
            "updated_at": "2026-10-29T18:15:44Z",
        }))
        .unwrap()
    }

    fn job(name: &str, steps: &serde_json::Value) -> Job {
        serde_json::from_value(serde_json::json!({
            "id": 123_626_890_371_u64,
            "name": name,
            "conclusion": "success",
            "started_at": "2026-10-29T17:52:55Z",
            "completed_at": "2026-10-29T17:55:16Z",
            "steps": steps,
        }))
        .unwrap()
    }

    fn run_check(conclusion: &str) -> serde_json::Value {
        serde_json::json!([
            {"name": "Set up job", "conclusion": "success", "started_at": "2026-10-29T17:52:55Z", "completed_at": "2026-10-29T17:52:56Z"},
            {"name": SKIP_CHECK, "conclusion": "skipped", "started_at": null, "completed_at": null},
            {"name": RUN_CHECK, "conclusion": conclusion, "started_at": "2026-10-29T17:53:25Z", "completed_at": "2026-10-29T17:54:31Z"},
        ])
    }

    fn nothing(_: &str) -> Results {
        Results::default()
    }

    #[test]
    fn a_run_of_ci_is_a_gate_named_for_the_commit_it_checked() {
        let run = run(GATE);

        let spans = spans(&run, &[], nothing);

        assert_eq!(run.build(), "7fc5a44 (2026-10-29)");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "gate");
        assert_eq!(spans[0].started_at.to_string(), "2026-10-29T17:47:29Z");
        assert_eq!(spans[0].ended_at.to_string(), "2026-10-29T18:15:44Z");
        assert_eq!(spans[0].fields["workflow"], GATE);
        assert_eq!(spans[0].key.as_deref(), Some("ci/run/40924676727/1"));
    }

    #[test]
    fn a_check_job_is_a_check_timed_as_its_check_with_what_it_uploaded() {
        let jobs = [job("checks / test", &run_check("success"))];
        let results = |check: &str| {
            assert_eq!(check, "test");
            Results {
                cache_status: Some("notBuilt".into()),
                tests: crate::junit::tests(
                    r#"<testsuites><testsuite><testcase name="t" classname="b" timestamp="2026-10-29T17:53:30Z" time="1.0"/></testsuite></testsuites>"#,
                ),
            }
        };

        let gate = spans(&run(GATE), &jobs, results).remove(0);

        let check = &gate.children[0];
        assert_eq!(check.name, "check");
        assert_eq!(check.fields["check"], "test");
        assert_eq!(check.fields["cache_status"], "notBuilt");
        assert_eq!(check.started_at.to_string(), "2026-10-29T17:53:25Z");
        assert_eq!(check.ended_at.to_string(), "2026-10-29T17:54:31Z");
        assert_eq!(check.outcome, Some(Outcome::Succeeded));
        assert_eq!(check.children.len(), 1);
    }

    #[test]
    fn a_check_unchanged_since_the_base_is_skipped_as_unchanged() {
        let steps = serde_json::json!([
            {"name": SKIP_CHECK, "conclusion": "success", "started_at": "2026-10-29T17:52:56Z", "completed_at": "2026-10-29T17:52:57Z"},
            {"name": RUN_CHECK, "conclusion": "skipped", "started_at": null, "completed_at": null},
        ]);

        let gate = spans(&run(GATE), &[job("lint", &steps)], nothing).remove(0);

        assert_eq!(gate.children[0].outcome, Some(Outcome::Skipped));
        assert_eq!(gate.children[0].fields["cache_status"], "unchanged");
    }

    #[test]
    fn a_failed_check_is_failed() {
        let gate = spans(&run(GATE), &[job("lint", &run_check("failure"))], nothing).remove(0);

        assert_eq!(gate.children[0].outcome, Some(Outcome::Failed));
        assert_eq!(gate.children[0].fields["conclusion"], "failure");
    }

    #[test]
    fn any_other_job_is_a_job_timed_as_the_job() {
        let gate = spans(
            &run(GATE),
            &[job("publish", &serde_json::json!([]))],
            nothing,
        )
        .remove(0);

        let job = &gate.children[0];
        assert_eq!(job.name, "job");
        assert_eq!(job.fields["job"], "publish");
        assert_eq!(job.started_at.to_string(), "2026-10-29T17:52:55Z");
    }

    #[test]
    fn another_workflow_leaves_its_jobs_alone_and_its_audit_as_a_check() {
        let steps = serde_json::json!([
            {"name": RUN_AUDIT, "conclusion": "success", "started_at": "2026-10-29T00:01:00Z", "completed_at": "2026-10-29T00:03:00Z"},
        ]);

        let spans = spans(
            &run("Security audit"),
            &[job("audit / Audit dependencies", &steps)],
            nothing,
        );

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "check");
        assert_eq!(spans[0].fields["check"], AUDIT);
        assert_eq!(spans[0].fields["workflow"], "Security audit");
    }
}
