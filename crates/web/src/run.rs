//! The run of `next` this server holds, and what every open page is shown of it.
//!
//! **The run belongs to the server, not to the request that started it.** A
//! page that is closed, or a phone that sleeps, costs the run nothing; a page
//! opened mid-run is shown where it has got to and then each step as it ends.
//! That is also what a run nobody pressed a button for will need.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use application::next::{Done, NextError, Progress, Step};
use configurator::{Credentials, next::NextFailure};
use domain::{normalised::OperatorZone, schedule::DayPart};
use maud::{Markup, PreEscaped, html};
use tokio::sync::watch;

use crate::report;

/// What the operator configured, which every run reads.
pub struct Setting {
    pub database: PathBuf,
    pub zone: OperatorZone,
    pub credentials: Credentials,
}

/// The latest run, as the page shows it.
#[derive(Clone, Default)]
pub struct View {
    /// Why the latest attempt did not start, above the run before it.
    refused: Option<String>,
    steps: Vec<Line>,
    /// The report, or why there is none. Rendered as the run ends, because a
    /// report is read once and not kept.
    ending: Option<PreEscaped<String>>,
}

/// One step: named as it starts, with its outcome once it ends.
#[derive(Clone)]
struct Line {
    step: Step,
    estimate: Option<String>,
    done: Option<Done>,
}

impl View {
    pub fn render(&self) -> Markup {
        html! {
            @if let Some(refused) = &self.refused { p.failed { (refused) } }
            @if !self.steps.is_empty() {
                ol.steps {
                    @for line in &self.steps {
                        li {
                            span.step { (report::step(&line.step)) }
                            " "
                            @match &line.done {
                                Some(done) => span.outcome.failed[done.outcome.is_err()] {
                                    (report::outcome(done))
                                },
                                None => span.outcome { "…" },
                            }
                            @if let Some(estimate) = &line.estimate {
                                div.estimate { (estimate) }
                            }
                        }
                    }
                }
            }
            @if let Some(ending) = &self.ending { (ending) }
        }
    }
}

/// The run this server holds, and whether one is going.
pub struct Runs {
    setting: Setting,
    view: watch::Sender<View>,
    going: AtomicBool,
}

impl Runs {
    pub fn new(setting: Setting) -> Self {
        Self {
            setting,
            view: watch::Sender::new(View::default()),
            going: AtomicBool::new(false),
        }
    }

    /// Each change to what the page shows, starting with how it stands now.
    pub fn watch(&self) -> watch::Receiver<View> {
        self.view.subscribe()
    }

    pub fn view(&self) -> View {
        self.view.borrow().clone()
    }

    /// Start a run, unless this server has one going.
    ///
    /// **A run going in the terminal is refused by the run itself**, which
    /// takes the store's lock before it reads anything, and the page is shown
    /// why. This only saves starting a second thread to be told the same.
    ///
    /// # Errors
    ///
    /// Why the run was not started, for the page to say.
    pub fn start(self: &Arc<Self>) -> Result<(), String> {
        if self.going.swap(true, Ordering::SeqCst) {
            return Err(NextError::AlreadyRunning.to_string());
        }
        let before = self.view.send_replace(View::default());

        let runs = Arc::clone(self);
        std::thread::Builder::new()
            .name("next".to_owned())
            .spawn(move || runs.run(before))
            .map(drop)
            .map_err(|error| {
                self.going.store(false, Ordering::SeqCst);
                format!("the run could not be started: {error}")
            })
    }

    /// The run, on a thread and a runtime of its own.
    ///
    /// **Its own runtime**, so nothing about the run's futures has to suit the
    /// server's: what is shared is the channel each step is sent down.
    ///
    /// **A run refused by the lock never happened**, so the page goes back to
    /// the run before it and says why this one did not start.
    fn run(&self, before: View) {
        let ending = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Err(error) => refused(&format!("the run could not be started: {error}")),
            Ok(runtime) => {
                let setting = &self.setting;
                let here = jiff::Timestamp::now().to_zoned(setting.zone.as_time_zone());
                let outcome = runtime.block_on(configurator::next::run(
                    &setting.database,
                    &setting.zone,
                    &setting.credentials,
                    DayPart::containing(here.datetime()),
                    None,
                    &Following(&self.view),
                ));
                match outcome {
                    Ok(answered) => report::report(&answered),
                    Err(failure @ NextFailure::Next(NextError::AlreadyRunning)) => {
                        self.view.send_replace(View {
                            refused: Some(failure.to_string()),
                            ..before
                        });
                        self.going.store(false, Ordering::SeqCst);
                        return;
                    }
                    Err(failure) => refused(&unanswered(&failure)),
                }
            }
        };
        self.view.send_modify(|view| view.ending = Some(ending));
        self.going.store(false, Ordering::SeqCst);
    }
}

/// Why the run could not answer, as the terminal says it.
fn unanswered(failure: &NextFailure) -> String {
    match failure {
        NextFailure::Next(NextError::NoWeek) => {
            format!("{failure}. Record one first: fitness schedule add")
        }
        other => other.to_string(),
    }
}

fn refused(message: &str) -> Markup {
    html! { p.failed { (message) } }
}

/// Progress, sent to every open page as each step starts and ends.
struct Following<'a>(&'a watch::Sender<View>);

impl Progress for Following<'_> {
    fn started(&self, step: &Step) {
        self.0.send_modify(|view| {
            view.steps.push(Line {
                step: step.clone(),
                estimate: None,
                done: None,
            });
        });
    }

    fn estimated(&self, step: &Step, outstanding: u64, takes: std::time::Duration) {
        self.0.send_modify(|view| {
            if let Some(line) = view
                .steps
                .iter_mut()
                .rev()
                .find(|line| line.step == *step && line.done.is_none())
            {
                line.estimate = report::estimate(outstanding, takes);
            }
        });
    }

    fn finished(&self, done: &Done) {
        self.0.send_modify(|view| {
            match view
                .steps
                .iter_mut()
                .rev()
                .find(|line| line.step == done.step && line.done.is_none())
            {
                Some(line) => line.done = Some(done.clone()),
                // A step reported only as it ended, as a walk with no access is.
                None => view.steps.push(Line {
                    step: done.step.clone(),
                    estimate: None,
                    done: Some(done.clone()),
                }),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, atomic::Ordering};

    use application::next::{Done, Found, NoAccess, Progress as _, Step, StepFailure};
    use domain::{landing::LandingStream, normalised::OperatorZone};
    use tokio::sync::watch;

    use super::{Following, Runs, Setting, View};

    fn shown(view: &watch::Sender<View>) -> String {
        view.borrow().render().into_string()
    }

    /// **A step is shown as it starts**, so a long one says what it is doing
    /// rather than looking hung, and its outcome replaces the ellipsis.
    #[test]
    fn a_step_is_shown_as_it_starts_and_finished_in_place() {
        let view = watch::Sender::new(View::default());
        let following = Following(&view);

        following.started(&Step::RefreshingClasses);
        following.estimated(
            &Step::RefreshingClasses,
            412,
            std::time::Duration::from_mins(10),
        );
        let started = shown(&view);
        assert!(
            started.contains("Refreshing Peloton's classes"),
            "{started}"
        );
        assert!(started.contains("…"), "{started}");
        assert!(
            started.contains("412 classes to read, about 10 minutes"),
            "{started}"
        );

        following.finished(&Done {
            step: Step::RefreshingClasses,
            outcome: Ok(Found::Classes {
                added: 3,
                read: 412,
                outstanding: 0,
                stopped: None,
            }),
        });
        let finished = shown(&view);
        assert!(finished.contains("done, 3 new classes"), "{finished}");
        assert!(!finished.contains("…"), "{finished}");
        assert_eq!(finished.matches("<li>").count(), 1, "{finished}");
    }

    /// A step reported only as it ended, as a walk with no access is, is
    /// still shown.
    #[test]
    fn a_step_reported_only_as_it_ended_is_shown() {
        let view = watch::Sender::new(View::default());
        let stream = LandingStream::try_from("hevy.workouts").expect("a stream");

        Following(&view).finished(&Done {
            step: Step::Fetching(stream),
            outcome: Err(StepFailure::NoAccess(NoAccess {
                detail: "no hevy credential".to_owned(),
            })),
        });
        let shown = shown(&view);
        assert!(shown.contains("Fetching hevy.workouts"), "{shown}");
        assert!(shown.contains("failed: no hevy credential"), "{shown}");
    }

    /// **A second press while a run is going is refused**, in the use case's
    /// words, and starts nothing.
    #[test]
    fn a_run_started_while_one_is_going_is_refused() {
        let runs = Arc::new(Runs::new(Setting {
            database: std::path::PathBuf::from("unused.db"),
            zone: OperatorZone::try_from("Europe/London").expect("a zone"),
            credentials: configurator::Credentials::default(),
        }));
        runs.going.store(true, Ordering::SeqCst);

        assert_eq!(
            runs.start(),
            Err("a run of next is already going".to_owned())
        );
        assert!(runs.view().steps.is_empty());
    }
}
