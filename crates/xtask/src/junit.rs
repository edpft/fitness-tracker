//! nextest's JUnit report, as `test` spans: each test's start, duration and
//! outcome, as nextest timed them.

use jiff::{SignedDuration, Timestamp};
use quick_xml::events::{BytesStart, Event};
use serde_json::Map;
use telemetry::{Outcome, Timed};

/// Every test case in `report`. One that cannot be read is left out: a report
/// half read records what it could.
pub fn tests(report: &str) -> Vec<Timed> {
    let mut reader = quick_xml::Reader::from_str(report);
    let mut tests = Vec::new();
    let mut open: Option<Timed> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Empty(case)) if case.name().as_ref() == "testcase" => {
                tests.extend(test(&case));
            }
            Ok(Event::Start(case)) if case.name().as_ref() == "testcase" => {
                open = test(&case);
            }
            Ok(Event::Start(inside) | Event::Empty(inside)) => {
                if let Some(test) = open.as_mut() {
                    match inside.name().as_ref() {
                        "failure" | "error" => test.outcome = Some(Outcome::Failed),
                        "skipped" => test.outcome = Some(Outcome::Skipped),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(end)) if end.name().as_ref() == "testcase" => {
                tests.extend(open.take());
            }
            Ok(Event::Eof) | Err(_) => return tests,
            Ok(_) => {}
        }
    }
}

fn test(case: &BytesStart<'_>) -> Option<Timed> {
    let attribute = |name: &str| {
        case.try_get_attribute(name)
            .ok()
            .flatten()
            .and_then(|attribute| attribute.normalized_value(quick_xml::XmlVersion::Explicit1_0).ok())
            .map(std::borrow::Cow::into_owned)
    };
    let started_at: Timestamp = attribute("timestamp")?.parse().ok()?;
    let seconds: f64 = attribute("time")?.parse().ok()?;
    let ended_at = started_at
        .checked_add(SignedDuration::try_from_secs_f64(seconds).ok()?)
        .ok()?;
    let mut fields = Map::new();
    fields.insert("binary".into(), attribute("classname")?.into());
    fields.insert("test".into(), attribute("name")?.into());
    Some(Timed {
        name: "test",
        key: None,
        started_at,
        ended_at,
        outcome: Some(Outcome::Succeeded),
        fields,
        children: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// As nextest 0.9.146 writes it, with a failure and a skip added.
    const REPORT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="3" skipped="1" failures="1" errors="0" timestamp="2026-10-29T18:00:52.580+00:00" time="0.174">
    <testsuite name="telemetry::recording" tests="3" skipped="1" errors="0" failures="1">
        <testcase name="nothing_recorded_creates_no_file" classname="telemetry::recording" timestamp="2026-10-29T18:00:52.605+00:00" time="0.007"/>
        <testcase name="a_step_is_the_child_of_the_run_it_was_in" classname="telemetry::recording" timestamp="2026-10-29T18:00:52.580+00:00" time="0.173">
            <failure type="test failure">thread panicked</failure>
        </testcase>
        <testcase name="ignored" classname="telemetry::recording" timestamp="2026-10-29T18:00:52.580+00:00" time="0.000">
            <skipped/>
        </testcase>
    </testsuite>
</testsuites>"#;

    #[test]
    fn each_test_case_is_a_test_span_timed_as_nextest_timed_it() {
        let tests = tests(REPORT);

        assert_eq!(tests.len(), 3);
        let first = &tests[0];
        assert_eq!(first.name, "test");
        assert_eq!(first.fields["binary"], "telemetry::recording");
        assert_eq!(first.fields["test"], "nothing_recorded_creates_no_file");
        assert_eq!(first.started_at.to_string(), "2026-10-29T18:00:52.605Z");
        assert_eq!(first.ended_at.to_string(), "2026-10-29T18:00:52.612Z");
        assert_eq!(first.outcome, Some(Outcome::Succeeded));
    }

    #[test]
    fn a_failed_test_and_a_skipped_one_say_so() {
        let tests = tests(REPORT);

        assert_eq!(tests[1].outcome, Some(Outcome::Failed));
        assert_eq!(tests[2].outcome, Some(Outcome::Skipped));
    }
}
