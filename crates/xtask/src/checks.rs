//! nix-fast-build's results, as `check` spans: one line per evaluation and per
//! build, read as it is written so each check is timed when it ends.
//!
//! Run with `--skip-cached`, which is what makes it report a check's cache
//! status at all (nix-fast-build 2.0.4). A check already in the store or in a
//! cache is not built, so it is recorded as skipped with no duration, and a
//! check fetched is never compared with one built.

use std::{collections::BTreeMap, path::Path};

use jiff::{SignedDuration, Timestamp};
use serde::Deserialize;
use serde_json::Map;
use telemetry::{Outcome, Timed};

#[derive(Debug, Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: String,
    attr: String,
    success: bool,
    duration: f64,
    #[serde(default)]
    outputs: BTreeMap<String, String>,
    #[serde(rename = "cacheStatus")]
    cache_status: Option<String>,
}

/// The check that `line`, read at `now`, ended, if it ended one: an evaluation
/// that failed or found the check already built, or a build.
///
/// `tests` reads the tests a built check ran, from its output.
pub fn ended(line: &str, now: Timestamp, tests: impl Fn(&Path) -> Vec<Timed>) -> Option<Timed> {
    let line: Line = serde_json::from_str(line).ok()?;
    let (started_at, outcome, cache_status, children) = match line.kind.as_str() {
        "EVAL" if !line.success => (now, Outcome::Failed, line.cache_status, Vec::new()),
        "EVAL" => match line.cache_status.as_deref() {
            Some("local" | "cached") => (now, Outcome::Skipped, line.cache_status, Vec::new()),
            _ => return None,
        },
        "BUILD" => {
            let took = SignedDuration::try_from_secs_f64(line.duration).ok()?;
            let children = match (line.success, line.outputs.get("out")) {
                (true, Some(out)) => tests(Path::new(out)),
                _ => Vec::new(),
            };
            let outcome = if line.success {
                Outcome::Succeeded
            } else {
                Outcome::Failed
            };
            // Only a check nothing had already built is built.
            (
                now.checked_sub(took).ok()?,
                outcome,
                Some("notBuilt".to_owned()),
                children,
            )
        }
        _ => return None,
    };
    let mut fields = Map::new();
    fields.insert("check".into(), line.attr.into());
    if let Some(cache_status) = cache_status {
        fields.insert("cache_status".into(), cache_status.into());
    }
    Some(Timed {
        name: "check",
        key: None,
        started_at,
        ended_at: now,
        outcome: Some(outcome),
        fields,
        children,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        "2026-10-29T18:00:00Z".parse().unwrap()
    }

    fn none(_: &Path) -> Vec<Timed> {
        Vec::new()
    }

    #[test]
    fn a_build_is_a_check_ending_when_it_is_read_and_as_long_as_it_took() {
        let line = r#"{"type": "BUILD", "attr": "lint", "success": true, "duration": 90.5, "error": null, "outputs": {"out": "/nix/store/x-lint"}}"#;

        let check = ended(line, now(), none).unwrap();

        assert_eq!(check.name, "check");
        assert_eq!(check.fields["check"], "lint");
        assert_eq!(check.fields["cache_status"], "notBuilt");
        assert_eq!(check.outcome, Some(Outcome::Succeeded));
        assert_eq!(check.started_at.to_string(), "2026-10-29T17:58:29.5Z");
        assert_eq!(check.ended_at, now());
    }

    #[test]
    fn a_failed_build_is_a_failed_check() {
        let line = r#"{"type": "BUILD", "attr": "x", "success": false, "duration": 1.0, "error": "build exited with 1", "outputs": {"out": "/nix/store/x"}}"#;

        assert_eq!(
            ended(line, now(), none).unwrap().outcome,
            Some(Outcome::Failed)
        );
    }

    #[test]
    fn a_check_already_built_is_skipped_with_where_it_was_found() {
        let line = r#"{"type": "EVAL", "attr": "typos", "success": true, "duration": 0.0, "error": null, "outputs": {"out": "/nix/store/y-typos"}, "drvPath": "/nix/store/y-typos.drv", "cacheStatus": "local"}"#;

        let check = ended(line, now(), none).unwrap();

        assert_eq!(check.outcome, Some(Outcome::Skipped));
        assert_eq!(check.fields["cache_status"], "local");
        assert_eq!(check.started_at, check.ended_at);
    }

    #[test]
    fn a_check_to_be_built_ends_with_its_build_and_not_before() {
        let line = r#"{"type": "EVAL", "attr": "lint", "success": true, "duration": 0.0, "error": null, "outputs": {"out": "/nix/store/x-lint"}, "drvPath": "/nix/store/x-lint.drv", "cacheStatus": "notBuilt"}"#;

        assert!(ended(line, now(), none).is_none());
    }

    #[test]
    fn a_check_that_does_not_evaluate_is_a_failed_check() {
        let line = r#"{"type": "EVAL", "attr": "lint", "success": false, "duration": 0.0, "error": "error: undefined variable"}"#;

        assert_eq!(
            ended(line, now(), none).unwrap().outcome,
            Some(Outcome::Failed)
        );
    }

    #[test]
    fn a_built_check_holds_the_tests_it_ran() {
        let line = r#"{"type": "BUILD", "attr": "test", "success": true, "duration": 1000.0, "error": null, "outputs": {"out": "/nix/store/z-test"}}"#;
        let tests = |out: &Path| {
            assert_eq!(out, Path::new("/nix/store/z-test"));
            vec![Timed {
                name: "test",
                key: None,
                started_at: now(),
                ended_at: now(),
                outcome: None,
                fields: Map::new(),
                children: Vec::new(),
            }]
        };

        assert_eq!(ended(line, now(), tests).unwrap().children.len(), 1);
    }
}
