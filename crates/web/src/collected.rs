//! Whether what each page reads has been collected, as a dot per stage (#666).
//!
//! **A row per thing the operator looks at**: each chart page, and the gym's
//! and cycling's sessions for the prescriptions. A row's parts are the scope
//! its page collects (#559), and a session's are what it is prescribed from.

use application::{
    collected::{Collected, Stage, collected},
    next::{Found, Scope, Step, StepFailure},
};
use configurator::wiring::WiringError;
use domain::planner::Discipline;
use maud::{Markup, html};

use crate::page::PAGES;

/// One row: what it is called, the page it links to, and what it reads.
struct Row {
    name: String,
    path: Option<&'static str>,
    reads: Result<Scope, WiringError>,
}

/// What the chart page at `path` collects, `None` for a page that is not a
/// chart.
fn chart(path: &str) -> Option<Result<Scope, WiringError>> {
    Some(match path {
        "/heart-rate" => configurator::heart_rate::collects(),
        "/ftp" => configurator::ftp_tests::collects(),
        "/body-weight" => configurator::body_weight::collects(),
        "/body-composition" => configurator::composition::collects(),
        "/resting-heart-rate" => configurator::per_device::resting_heart_rate_collects(),
        "/hrv" => configurator::per_device::overnight_hrv_collects(),
        "/pulse-wave-velocity" => configurator::pulse_wave_velocity::collects(),
        "/sleep" => configurator::sleep::collects(),
        "/estimated-1rm" => configurator::one_rep_max::collects(),
        _ => return None,
    })
}

/// The sessions first, then the charts in the menu's order.
fn rows() -> Vec<Row> {
    let sessions = Discipline::ALL.iter().map(|discipline| Row {
        name: format!("{} sessions", capitalised(discipline.as_str())),
        path: None,
        reads: configurator::next::prescribed_from(*discipline),
    });
    let charts = PAGES.iter().filter_map(|(path, name)| {
        chart(path).map(|reads| Row {
            name: (*name).to_owned(),
            path: Some(path),
            reads,
        })
    });
    sessions.chain(charts).collect()
}

/// Every row, a column per stage, from the steps of this server's latest runs.
pub fn matrix<'a>(
    steps: &(impl IntoIterator<Item = (&'a Step, Option<&'a Result<Found, StepFailure>>)> + Clone),
    going: bool,
) -> Markup {
    html! {
        section.card.collected {
            div.row.head {
                span {}
                span { "Fetch" }
                span { "Normalise" }
                span { "Canon" (maud::PreEscaped("&shy;")) "icalise" }
            }
            @for row in rows() {
                div.row {
                    @match row.path {
                        Some(path) => a href=(path) { (row.name) },
                        None => span { (row.name) },
                    }
                    @match &row.reads {
                        Ok(reads) => {
                            @for stage in Stage::ALL {
                                @match collected(reads, stage, steps, going) {
                                    Some(standing) => (dot(standing)),
                                    None => span {},
                                }
                            }
                        }
                        Err(error) => span.failed.unread { (error) },
                    }
                }
            }
            p.key {
                span { (dot(Collected::Done)) "done" }
                span { (dot(Collected::Failed)) "failed, why is in telemetry" }
                span { (dot(Collected::NotDue)) "not due" }
                span { (dot(Collected::Unfinished)) "not finished" }
            }
        }
    }
}

fn dot(standing: Collected) -> Markup {
    let (class, said) = match standing {
        Collected::Done => ("done", "done"),
        Collected::Failed => ("failed", "failed"),
        Collected::Unfinished => ("unfinished", "not finished"),
        Collected::NotDue => ("idle", "not due"),
        Collected::NotRun => ("idle", "not run"),
    };
    html! { i class={ "dot " (class) } role="img" aria-label=(said) title=(said) {} }
}

fn capitalised(word: &str) -> String {
    let mut letters = word.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(letters).collect()
    })
}

#[cfg(test)]
mod tests {
    use application::next::{Found, NoAccess, Step, StepFailure};
    use domain::landing::LandingStream;

    use super::{chart, matrix, rows};
    use crate::page::PAGES;

    /// **A row for each session and each chart page**, every one of which
    /// reads something.
    #[test]
    fn a_row_for_each_session_and_each_chart() {
        let rows = rows();
        let names: Vec<_> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names[..2], ["Gym sessions", "Cycling sessions"]);
        assert_eq!(
            rows.len(),
            2 + PAGES
                .iter()
                .filter(|(path, _)| chart(path).is_some())
                .count()
        );
        assert_eq!(rows.len(), 11, "{names:?}");
        for row in &rows {
            assert!(row.reads.is_ok(), "{}", row.name);
        }
    }

    /// **On a store with no credentials every fetch is red**, sleep has no
    /// canonicalise, and a stage the run has not reached pulses.
    #[test]
    fn no_credentials_reds_the_fetches_and_sleep_has_no_canonicalise() {
        let failed: Result<Found, StepFailure> = Err(StepFailure::NoAccess(NoAccess {
            detail: "no credential".to_owned(),
        }));
        let fetches: Vec<Step> = configurator::catalogue::KNOWN
            .iter()
            .filter_map(|known| LandingStream::try_from(known.name().as_str()).ok())
            .map(Step::Fetching)
            .collect();
        let steps: Vec<_> = fetches.iter().map(|step| (step, Some(&failed))).collect();
        let shown = matrix(&steps.iter().copied(), true).into_string();

        let rows: Vec<&str> = shown.split(r#"<div class="row">"#).skip(1).collect();
        assert_eq!(rows.len(), 11, "{shown}");
        for row in &rows {
            let dots: Vec<&str> = row.split(r#"<i class="dot "#).skip(1).collect();
            assert!(dots[0].starts_with("failed"), "{row}");
            assert!(dots[1].starts_with("unfinished"), "{row}");
        }
        let sleep = rows
            .iter()
            .find(|row| row.contains(">Sleep<"))
            .expect("a sleep row");
        assert_eq!(sleep.matches(r#"<i class="dot "#).count(), 2, "{sleep}");
        assert!(
            rows[0].starts_with("<span>Gym sessions</span>"),
            "{}",
            rows[0]
        );
    }
}
