//! Informational telemetry (#631): where `telemetry.db` is, which build its
//! spans are stamped with, and the spans this crate opens around the use cases
//! it composes.

use std::path::Path;

use application::{
    CanonicalLayer, DerivedFrom, Rederivation, StoreError,
    canonicalise::{Canonicalised, WeighInsCanonicalised},
    next::{Canonicalising, Done, Found, IfGoing, NotDue, Progress, Refetch, Step, StepFailure},
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
        ::telemetry::record(&store.with_file_name(FILE), crate::next::named_build());
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
    async fn gym(&self, rederivation: Rederivation) -> Result<Canonicalised, StoreError> {
        let span = canonicalising(CanonicalLayer::Gym);
        let done = self.0.gym(rederivation).instrument(span.clone()).await;
        ended(&span, &done, counts);
        done
    }

    async fn cycling(&self, rederivation: Rederivation) -> Result<Canonicalised, StoreError> {
        let span = canonicalising(CanonicalLayer::Cycling);
        let done = self.0.cycling(rederivation).instrument(span.clone()).await;
        ended(&span, &done, counts);
        done
    }

    async fn weigh_ins(
        &self,
        rederivation: Rederivation,
    ) -> Result<WeighInsCanonicalised, StoreError> {
        let span = canonicalising(CanonicalLayer::WeighIns);
        let done = self
            .0
            .weigh_ins(rederivation)
            .instrument(span.clone())
            .await;
        ended(&span, &done, |done| {
            (done.read.as_usize(), done.written.as_usize())
        });
        done
    }

    async fn inputs(&self, layer: CanonicalLayer) -> Result<DerivedFrom, StoreError> {
        self.0.inputs(layer).await
    }
}

/// A chart page load as a `request` span (#628).
///
/// Its page, and whether it is a redraw: the page asking for itself again as
/// htmx. Its range and how it was served are recorded as the page learns
/// them, and a run it starts is its child.
pub fn request(page: &str, redraw: bool) -> Span {
    tracing::info_span!(
        "request",
        page,
        range = Empty,
        served = Empty,
        redraw,
        joined = Empty,
        due = Empty,
        outcome = Empty,
    )
}

/// How a chart page was served.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Served {
    /// As it was last drawn, while a run collects what it reads: fast, but
    /// not yet whole.
    Kept,
    /// The placeholder, where it has never been drawn on this store.
    Placeholder,
    /// Drawn from the store.
    Drawn,
}

impl Served {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Kept => "kept",
            Self::Placeholder => "placeholder",
            Self::Drawn => "drawn",
        }
    }
}

/// The range of the page being loaded, where it has one, on its span.
pub fn ranged(range: &str) {
    Span::current().record("range", range);
}

/// How the page being loaded was served, on its span.
pub fn served(served: Served) {
    Span::current().record("served", served.as_str());
}

/// The page being loaded joined a run this frontend already had going, so it
/// started none of its own.
pub fn joined() {
    Span::current().record("joined", true);
}

/// The page being loaded asked for nothing that could be new, so it started
/// no run and joined none (#612).
pub fn not_due() {
    Span::current().record("due", false);
}

/// How a page load ended, on its span.
pub fn request_ended(span: &Span, succeeded: bool) {
    let outcome = if succeeded {
        Outcome::Succeeded
    } else {
        Outcome::Failed
    };
    span.record(OUTCOME, outcome.as_str());
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
    /// How soon a stream fetched a moment ago is fetched again (#612): a page
    /// waits out the shortest visit, and the terminal asks whenever it is run.
    pub const fn refetch(self) -> Refetch {
        match self {
            Self::Cli => Refetch::WhenDue,
            Self::Web => Refetch::SHORTEST_VISIT,
        }
    }
}

/// A run of `next` from the moment it is asked for: its frontend, and a `run`
/// span (#632) with what it covered, and whether it joined another run or was
/// refused because one was going.
///
/// **Made where it is asked for, not where it runs** (#628): the span follows
/// from whatever was going when it was made, a page load for a page, so the
/// run is recorded as that load's child although it outlives it.
pub struct Run {
    frontend: Frontend,
    pub(crate) span: Span,
}

impl Run {
    /// The whole of `next`.
    pub fn everything(frontend: Frontend) -> Self {
        Self::new(frontend, "everything")
    }

    /// Only what `scope` reads, as a chart page asks for.
    pub fn over(frontend: Frontend, scope: &application::next::Scope) -> Self {
        Self::new(frontend, &covered(scope.parts()))
    }

    fn new(frontend: Frontend, covered: &str) -> Self {
        let span = tracing::info_span!(
            parent: None,
            "run",
            frontend = frontend.as_str(),
            covered,
            joined = Empty,
            refused = Empty,
            outcome = Empty,
        );
        span.follows_from(Span::current());
        Self { frontend, span }
    }

    pub(crate) const fn if_going(&self) -> IfGoing {
        self.frontend.if_going()
    }

    pub(crate) const fn refetch(&self) -> Refetch {
        self.frontend.refetch()
    }
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
fn covered(parts: &std::collections::BTreeSet<application::Part>) -> String {
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
            why = Empty,
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
        Ok(Found::NotDue(why)) => {
            span.record("found", "not due");
            span.record(
                "why",
                match why {
                    NotDue::UntilTomorrow => "until tomorrow",
                    NotDue::Fetched { .. } => "fetched",
                    NotDue::Follows(_) => "follows",
                    NotDue::NoSession(_) => "no session",
                },
            );
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
                Found::NotDue(_) | Found::Unchanged => {}
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
        CanonicalLayer, DerivedFrom, Part, Rederivation, StoreError,
        canonicalise::{Canonicalised, WeighInsCanonicalised},
        next::{Canonicalising, Done, Found, NoAccess, Progress, Step, StepFailure},
    };
    use domain::{canonical::SessionCount, landing::LandingStream, schedule::Discipline};
    use sqlx::{Connection as _, Row as _};
    use tracing_subscriber::layer::SubscriberExt as _;

    use super::{Frontend, Run, Served, Spanned, Traced};

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

    /// A layer that reads three and writes two, and fails nothing else.
    struct Rebuilt;

    impl Canonicalising for Rebuilt {
        async fn gym(&self, _: Rederivation) -> Result<Canonicalised, StoreError> {
            Ok(Canonicalised {
                read: SessionCount::from(3),
                written: SessionCount::from(2),
            })
        }

        async fn cycling(&self, _: Rederivation) -> Result<Canonicalised, StoreError> {
            Err(StoreError::Unavailable {
                detail: "gone".to_owned(),
            })
        }

        async fn weigh_ins(&self, _: Rederivation) -> Result<WeighInsCanonicalised, StoreError> {
            Err(StoreError::Unavailable {
                detail: "gone".to_owned(),
            })
        }

        async fn inputs(&self, _: CanonicalLayer) -> Result<DerivedFrom, StoreError> {
            Err(StoreError::Unavailable {
                detail: "gone".to_owned(),
            })
        }
    }

    /// **A rebuild is a `step` span with its layer and what it read and
    /// wrote**, timed by the span, so the store keeps no log of it (#633).
    #[test]
    fn a_rebuild_is_a_span_with_what_it_read_and_wrote() {
        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let database = directory.path().join("telemetry.db");

        let (recorder, recording) = ::telemetry::record(&database, None);
        tracing::subscriber::with_default(tracing_subscriber::registry().with(recorder), || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .expect("a runtime");
            runtime.block_on(async {
                let traced = Traced(Rebuilt);
                traced.gym(Rederivation::Whole).await.expect("a rebuild");
                let _ = traced.cycling(Rederivation::Whole).await;
            });
        });
        drop(recording);

        let rows = spans(&database).expect("telemetry.db reads");
        let step = |layer: &str| {
            rows.iter()
                .find(|row| row.name == "step" && row.fields["layer"] == layer)
                .unwrap_or_else(|| panic!("a {layer} step"))
        };
        let gym = step("gym");
        assert_eq!(gym.outcome.as_deref(), Some("succeeded"));
        assert_eq!(
            gym.fields,
            serde_json::json!({
                "step": "canonicalising",
                "layer": "gym",
                "read": 3,
                "written": 2,
            })
        );
        assert_eq!(step("cycling").outcome.as_deref(), Some("failed"));
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
            let spanned = Spanned::new(&heard, Run::everything(Frontend::Cli).span);
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

    /// **A run a page load starts is that load's child, and the load ends as
    /// it is served** (#628), with its page, range and how it was served.
    #[test]
    fn a_run_a_page_load_starts_is_its_child() {
        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let database = directory.path().join("telemetry.db");
        let hrv = application::next::Scope::new([Part::Normalising(
            LandingStream::try_from("garmin.hrv").expect("a stream name"),
        )]);

        let (recorder, recording) = ::telemetry::record(&database, None);
        tracing::subscriber::with_default(tracing_subscriber::registry().with(recorder), || {
            let request = super::request("/hrv", false);
            let run = request.in_scope(|| {
                super::ranged("macrocycle");
                super::served(Served::Kept);
                Run::over(Frontend::Web, &hrv)
            });
            super::request_ended(&request, true);
            drop(request);
            std::thread::sleep(Duration::from_millis(20));
            drop(run);
        });
        drop(recording);

        let rows = spans(&database).expect("telemetry.db reads");
        let request = rows
            .iter()
            .find(|row| row.name == "request")
            .expect("a request");
        let run = rows.iter().find(|row| row.name == "run").expect("a run");
        assert_eq!(run.parent, Some(request.id));
        assert_eq!(run.fields["covered"], "normalising garmin.hrv");
        assert_eq!(request.parent, None);
        assert_eq!(request.outcome.as_deref(), Some("succeeded"));
        assert_eq!(
            request.fields,
            serde_json::json!({
                "page": "/hrv",
                "range": "macrocycle",
                "served": "kept",
                "redraw": false,
            })
        );
    }
}
