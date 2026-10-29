//! Informational telemetry (#631): where `telemetry.db` is, which build its
//! spans are stamped with, and the spans this crate opens around the use cases
//! it composes.

use std::path::Path;

use application::{
    CanonicalLayer, DerivedFrom, StoreError,
    canonicalise::{Canonicalised, WeighInsCanonicalised},
    next::{Canonicalising, Done, Found, IfGoing, Progress, Step, StepFailure},
};
use tracing::{Instrument as _, Span, field::Empty};
use tracing_subscriber::layer::SubscriberExt as _;

pub use ::telemetry::Recording;
use ::telemetry::{OUTCOME, Outcome};

/// The file, beside the store and never inside it: the store's migrations are
/// not its migrations, and losing it costs history and nothing else.
const FILE: &str = "telemetry.db";

/// Record this process's spans to the `telemetry.db` beside `store`, until the
/// [`Recording`] is dropped.
///
/// **A second call records nothing.** One subscriber serves a process, so
/// whichever store was named first is where the spans go.
pub fn record(store: &Path) -> Recording {
    let (recorder, recording) =
        ::telemetry::record(&store.with_file_name(FILE), crate::next::BUILD);
    let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry().with(recorder));
    recording
}

/// Each rebuild as a `step` span: its layer, and what it read and wrote.
pub struct Traced<C>(pub C);

fn canonicalising(layer: CanonicalLayer) -> Span {
    tracing::info_span!(
        "step",
        step = "canonicalising",
        layer = layer.as_str(),
        read = Empty,
        written = Empty,
        outcome = Empty,
    )
}

fn ended<T>(span: &Span, done: &Result<T, StoreError>, counts: impl Fn(&T) -> (usize, usize)) {
    match done {
        Ok(done) => {
            let (read, written) = counts(done);
            span.record("read", read);
            span.record("written", written);
            span.record(OUTCOME, Outcome::Succeeded.as_str());
        }
        Err(_) => {
            span.record(OUTCOME, Outcome::Failed.as_str());
        }
    }
}

const fn counts(done: &Canonicalised) -> (usize, usize) {
    (done.read.as_usize(), done.written.as_usize())
}

impl<C: Canonicalising + Sync> Canonicalising for Traced<C> {
    async fn gym(&self) -> Result<Canonicalised, StoreError> {
        let span = canonicalising(CanonicalLayer::Gym);
        let done = self.0.gym().instrument(span.clone()).await;
        ended(&span, &done, counts);
        done
    }

    async fn cycling(&self) -> Result<Canonicalised, StoreError> {
        let span = canonicalising(CanonicalLayer::Cycling);
        let done = self.0.cycling().instrument(span.clone()).await;
        ended(&span, &done, counts);
        done
    }

    async fn weigh_ins(&self) -> Result<WeighInsCanonicalised, StoreError> {
        let span = canonicalising(CanonicalLayer::WeighIns);
        let done = self.0.weigh_ins().instrument(span.clone()).await;
        ended(&span, &done, |done| {
            (done.read.as_usize(), done.written.as_usize())
        });
        done
    }

    async fn inputs(&self, layer: CanonicalLayer) -> Result<DerivedFrom, StoreError> {
        self.0.inputs(layer).await
    }
}

/// Which frontend started a run of `next`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frontend {
    Cli,
    Web,
}

impl Frontend {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Web => "web",
        }
    }

    /// What its run does when another is going: the terminal says so at once,
    /// and a page joins it.
    pub const fn if_going(self) -> IfGoing {
        match self {
            Self::Cli => IfGoing::Refuse,
            Self::Web => IfGoing::Join,
        }
    }
}

/// A run of `next` as a `run` span (#632): its frontend, what it covered, and
/// whether it joined another run or was refused because one was going.
pub(crate) fn run(frontend: Frontend, covered: &str) -> Span {
    tracing::info_span!(
        "run",
        frontend = frontend.as_str(),
        covered,
        joined = Empty,
        refused = Empty,
        outcome = Empty,
    )
}

/// How a run ended, on its span.
pub(crate) fn run_ended<T>(span: &Span, done: &Result<T, crate::next::NextFailure>) {
    use crate::next::NextFailure;
    use application::next::NextError;
    let outcome = match done {
        Ok(_) | Err(NextFailure::Next(NextError::Joined)) => Outcome::Succeeded,
        Err(NextFailure::Next(NextError::AlreadyRunning)) => {
            span.record("refused", true);
            Outcome::Failed
        }
        Err(_) => Outcome::Failed,
    };
    span.record(OUTCOME, outcome.as_str());
}

/// What a run covered: everything, or the parts a page collects.
pub(crate) fn covered(parts: &std::collections::BTreeSet<application::Part>) -> String {
    use application::Part;
    parts
        .iter()
        .map(|part| match part {
            Part::Fetching(stream) => format!("fetching {stream}"),
            Part::Normalising(stream) => format!("normalising {stream}"),
            Part::Canonicalising(layer) => format!("canonicalising {}", layer.as_str()),
            Part::Delivering => "delivering".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The run's [`Progress`], passed on, and each step as a `step` span whose
/// parent is the run's (#632). A step skipped as not due or unchanged is
/// recorded as `skipped`.
pub(crate) struct Spanned<'a, P> {
    pub(crate) progress: &'a P,
    pub(crate) run: Span,
    open: std::sync::Mutex<Vec<(Step, Span)>>,
}

impl<'a, P> Spanned<'a, P> {
    pub(crate) const fn new(progress: &'a P, run: Span) -> Self {
        Self {
            progress,
            run,
            open: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn open(&self) -> std::sync::MutexGuard<'_, Vec<(Step, Span)>> {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn step(&self, step: &Step) -> Span {
        let span = tracing::info_span!(
            parent: &self.run,
            "step",
            step = Empty,
            stream = Empty,
            layer = Empty,
            parts = Empty,
            found = Empty,
            landed = Empty,
            written = Empty,
            left_out = Empty,
            added = Empty,
            read = Empty,
            outstanding = Empty,
            stopped = Empty,
            failure = Empty,
            outcome = Empty,
        );
        let name = match step {
            Step::Joining(parts) => {
                span.record("parts", covered(parts));
                "joining"
            }
            Step::Fetching(stream) => {
                span.record("stream", stream.to_string());
                "fetching"
            }
            Step::Normalising(stream) => {
                span.record("stream", stream.to_string());
                "normalising"
            }
            Step::Canonicalising(discipline) => {
                span.record("layer", discipline.as_str());
                "canonicalising"
            }
            Step::CanonicalisingWeighIns => {
                span.record("layer", CanonicalLayer::WeighIns.as_str());
                "canonicalising"
            }
            Step::RefreshingClasses => "refreshing classes",
            Step::Committing => "committing",
            Step::Skipping => "skipping",
        };
        span.record("step", name);
        span
    }
}

/// What a step found, on its span, and how it ended.
fn found(span: &Span, outcome: &Result<Found, StepFailure>) {
    let ended = match outcome {
        Err(failure) => {
            span.record("failure", failure.to_string());
            Outcome::Failed
        }
        Ok(Found::NotDue) => {
            span.record("found", "not due");
            Outcome::Skipped
        }
        Ok(Found::Unchanged) => {
            span.record("found", "unchanged");
            Outcome::Skipped
        }
        Ok(found) => {
            match found {
                Found::Joined => {
                    span.record("found", "joined");
                }
                Found::Landed(landed) => {
                    span.record("landed", landed.as_usize());
                }
                Found::Written(written) => {
                    span.record("written", written.as_usize());
                }
                Found::Canonical(written) => {
                    span.record("written", written.as_usize());
                }
                Found::CanonicalWeighIns { written, left_out } => {
                    span.record("written", written.as_usize());
                    span.record("left_out", left_out.len());
                }
                Found::Classes {
                    added,
                    read,
                    outstanding,
                    stopped,
                } => {
                    span.record("added", added);
                    span.record("read", read);
                    span.record("outstanding", outstanding);
                    if let Some(stopped) = stopped {
                        span.record("stopped", stopped.to_string());
                    }
                }
                Found::Committed { .. } => {
                    span.record("found", "committed");
                }
                Found::Skipped { .. } => {
                    span.record("found", "skipped");
                }
                Found::NotDue | Found::Unchanged => {}
            }
            Outcome::Succeeded
        }
    };
    span.record(OUTCOME, ended.as_str());
}

impl<P: Progress + Sync> Progress for Spanned<'_, P> {
    fn started(&self, step: &Step) {
        let span = self.step(step);
        self.open().push((step.clone(), span));
        self.progress.started(step);
    }

    fn estimated(&self, step: &Step, outstanding: u64, takes: std::time::Duration) {
        self.progress.estimated(step, outstanding, takes);
    }

    fn finished(&self, done: &Done) {
        let started = {
            let mut open = self.open();
            open.iter()
                .rposition(|(step, _)| *step == done.step)
                .map(|at| open.remove(at).1)
        };
        // A step reported only as it ended, as a walk with no access is.
        let span = started.unwrap_or_else(|| self.step(&done.step));
        if matches!(done.step, Step::Joining(_)) {
            self.run.record("joined", true);
        }
        found(&span, &done.outcome);
        drop(span);
        self.progress.finished(done);
    }
}

#[cfg(test)]
mod tests {
    use std::{path::Path, time::Duration};

    use application::{
        Part,
        next::{Done, Found, NoAccess, Progress, Step, StepFailure},
    };
    use domain::{landing::LandingStream, schedule::Discipline};
    use sqlx::{Connection as _, Row as _};
    use tracing_subscriber::layer::SubscriberExt as _;

    use super::{Frontend, Spanned, run};

    /// A frontend's progress, which hears nothing it would not have.
    #[derive(Default)]
    struct Heard(std::sync::Mutex<Vec<String>>);

    impl Progress for Heard {
        fn started(&self, step: &Step) {
            if let Ok(mut heard) = self.0.lock() {
                heard.push(format!("started {step:?}"));
            }
        }

        fn estimated(&self, _: &Step, _: u64, _: Duration) {}

        fn finished(&self, done: &Done) {
            if let Ok(mut heard) = self.0.lock() {
                heard.push(format!("finished {:?}", done.step));
            }
        }
    }

    struct Row {
        id: i64,
        parent: Option<i64>,
        name: String,
        outcome: Option<String>,
        fields: serde_json::Value,
    }

    fn spans(database: &Path) -> Result<Vec<Row>, Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(async {
            let options = sqlx::sqlite::SqliteConnectOptions::new().filename(database);
            let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
            let rows = sqlx::query("SELECT id, parent, name, outcome, fields FROM span")
                .fetch_all(&mut connection)
                .await?;
            rows.into_iter()
                .map(|row| {
                    Ok(Row {
                        id: row.try_get("id")?,
                        parent: row.try_get("parent")?,
                        name: row.try_get("name")?,
                        outcome: row.try_get("outcome")?,
                        fields: serde_json::from_str(row.try_get("fields")?)?,
                    })
                })
                .collect()
        })
    }

    fn done(step: Step, outcome: Result<Found, StepFailure>) -> Done {
        Done {
            step,
            outcome,
            took: Duration::ZERO,
        }
    }

    /// **Every step its run reports is a `step` span under the run's**, the
    /// skipped and the one reported only as it ended included, and the
    /// frontend hears what it heard before (#632).
    #[test]
    fn each_step_reported_is_a_span_under_its_run() {
        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let database = directory.path().join("telemetry.db");
        let hevy = LandingStream::try_from("hevy.workouts").expect("a stream name");
        let heard = Heard::default();

        let (recorder, recording) = ::telemetry::record(&database, None);
        tracing::subscriber::with_default(tracing_subscriber::registry().with(recorder), || {
            let spanned = Spanned::new(&heard, run(Frontend::Cli, "everything"));
            let joined = Step::Joining([Part::Delivering].into());
            spanned.started(&joined);
            spanned.finished(&done(joined, Ok(Found::Joined)));
            spanned.started(&Step::Normalising(hevy.clone()));
            spanned.finished(&done(Step::Normalising(hevy.clone()), Ok(Found::Unchanged)));
            spanned.started(&Step::Canonicalising(Discipline::Gym));
            spanned.finished(&done(
                Step::Canonicalising(Discipline::Gym),
                Ok(Found::Unchanged),
            ));
            spanned.finished(&done(
                Step::Fetching(hevy.clone()),
                Err(StepFailure::NoAccess(NoAccess {
                    detail: "no key".to_owned(),
                })),
            ));
        });
        drop(recording);

        let rows = spans(&database).expect("telemetry.db reads");
        let runs: Vec<_> = rows.iter().filter(|row| row.name == "run").collect();
        assert_eq!(runs.len(), 1);
        let run = runs[0];
        assert_eq!(run.fields["frontend"], "cli");
        assert_eq!(run.fields["covered"], "everything");
        assert_eq!(run.fields["joined"], true);

        let steps: Vec<_> = rows.iter().filter(|row| row.name == "step").collect();
        assert_eq!(steps.len(), 4);
        assert!(steps.iter().all(|step| step.parent == Some(run.id)));
        let step = |name: &str| {
            steps
                .iter()
                .find(|step| step.fields["step"] == name)
                .unwrap_or_else(|| panic!("a {name} step"))
        };
        assert_eq!(step("joining").fields["parts"], "delivering");
        let normalising = step("normalising");
        assert_eq!(normalising.fields["stream"], "hevy.workouts");
        assert_eq!(normalising.fields["found"], "unchanged");
        assert_eq!(normalising.outcome.as_deref(), Some("skipped"));
        assert_eq!(step("canonicalising").fields["layer"], "gym");
        let fetching = step("fetching");
        assert_eq!(fetching.outcome.as_deref(), Some("failed"));
        assert_eq!(fetching.fields["failure"], "no key");

        let heard = heard.0.into_inner().expect("unpoisoned");
        assert_eq!(heard.len(), 7, "{heard:?}");
    }
}
