//! The run of `next` this server holds, and what every open page is shown of it.
//!
//! **The run belongs to the server, not to the request that started it.** A
//! page that is closed, or a phone that sleeps, costs the run nothing; a page
//! opened mid-run is shown where it has got to and then each step as it ends.
//! Opening a page is what starts one (#466), so nobody presses anything.

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use application::next::{Done, NextError, Progress, Step};
use configurator::next::NextFailure;
use domain::{normalised::OperatorZone, schedule::DayPart};
use maud::{Markup, PreEscaped, html};
use tokio::sync::watch;

use crate::report;

/// What the operator configured, which every run reads.
pub struct Setting {
    pub database: PathBuf,
    pub zone: OperatorZone,
    /// Where the credentials are kept, `None` where nothing says.
    ///
    /// **A place, not what was in it** (#438): each run reads the file as it
    /// stands, so a credential stored while the server runs is the one the
    /// next run uses.
    pub credentials: Option<PathBuf>,
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
    going: bool,
    /// How many runs have ended since the server started, so a page drawn
    /// from the store knows whether a run has ended since it was drawn.
    ///
    /// **A count rather than a flag**, because a page is sent the latest view
    /// and not every view: a run that starts and ends between two of them
    /// would leave a flag where it found it.
    ended: u64,
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

    /// The step the run is on, for a page that shows the store rather than
    /// the run: nothing once it ends, and why it did not start if it did not.
    pub fn step(&self) -> Markup {
        html! {
            @if let Some(refused) = &self.refused { p.failed { (refused) } }
            @if self.going {
                @if let Some(line) = self.steps.last() {
                    p {
                        span.step { (report::step(&line.step)) }
                        " "
                        @match &line.done {
                            Some(done) => span.outcome.failed[done.outcome.is_err()] {
                                (report::outcome(done))
                            },
                            None => span.outcome { "…" },
                        }
                    }
                }
            }
        }
    }

    pub const fn ended(&self) -> u64 {
        self.ended
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

    /// What the operator configured.
    pub const fn setting(&self) -> &Setting {
        &self.setting
    }

    pub fn view(&self) -> View {
        self.view.borrow().clone()
    }

    /// Start a run, unless this server has one going, in which case the page
    /// opening joins it.
    ///
    /// **A run going in the terminal is refused by the run itself**, which
    /// takes the store's lock before it reads anything, and every page is
    /// shown why. This only saves starting a second thread to be told the same.
    /// So is a run that could not be started at all: there is no one request
    /// to answer, so it is said where every page will see it.
    pub fn start(self: &Arc<Self>) {
        if self.going.swap(true, Ordering::SeqCst) {
            return;
        }
        let ended = self.view.borrow().ended;
        let before = self.view.send_replace(View {
            going: true,
            ended,
            ..View::default()
        });

        let runs = Arc::clone(self);
        if let Err(error) = std::thread::Builder::new()
            .name("next".to_owned())
            .spawn(move || runs.run(before))
        {
            self.view.send_modify(|view| {
                view.going = false;
                view.refused = Some(format!("the run could not be started: {error}"));
            });
            self.going.store(false, Ordering::SeqCst);
        }
    }

    /// The run, on a thread and a runtime of its own.
    ///
    /// **Its own runtime**, so nothing about the run's futures has to suit the
    /// server's: what is shared is the channel each step is sent down.
    ///
    /// **A run refused by the lock never happened**, so the page goes back to
    /// the run before it and says why this one did not start, and nothing
    /// drawn from the store is redrawn.
    fn run(&self, before: View) {
        let ending = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Err(error) => refused(&format!("the run could not be started: {error}")),
            Ok(runtime) => {
                let setting = &self.setting;
                let credentials =
                    match configurator::config::credentials_at(setting.credentials.as_deref()) {
                        Ok(credentials) => credentials,
                        Err(error) => {
                            return self
                                .end(refused(&format!("the run could not be started: {error}")));
                        }
                    };
                let here = jiff::Timestamp::now().to_zoned(setting.zone.as_time_zone());
                let outcome = runtime.block_on(configurator::next::run(
                    &setting.database,
                    &setting.zone,
                    &credentials,
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
        self.end(ending);
    }

    /// The run is over: every page is shown how it ended.
    fn end(&self, ending: Markup) {
        self.view.send_modify(|view| {
            view.ending = Some(ending);
            view.going = false;
            view.ended += 1;
        });
        self.going.store(false, Ordering::SeqCst);
    }
}

/// What a page drawn from the store is sent of a change to the run: `ended`
/// the first time it sees a run has ended since `since`, so it redraws once
/// for each, and `step` otherwise.
pub fn step_event(since: &mut u64, view: &View) -> (&'static str, Markup) {
    let name = if view.ended > *since {
        *since = view.ended;
        "ended"
    } else {
        "step"
    };
    (name, view.step())
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

    use super::{Following, Runs, Setting, View, step_event};

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

    /// **A page opened while a run is going joins it**: nothing starts, and
    /// the page is shown where the run has got to.
    #[test]
    fn a_page_opened_mid_run_joins_it() {
        let runs = Arc::new(Runs::new(Setting {
            database: std::path::PathBuf::from("unused.db"),
            zone: OperatorZone::try_from("Europe/London").expect("a zone"),
            credentials: None,
        }));
        runs.going.store(true, Ordering::SeqCst);
        runs.view.send_modify(|view| view.going = true);
        Following(&runs.view).started(&Step::RefreshingClasses);

        runs.start();

        let view = runs.view();
        assert_eq!(view.steps.len(), 1);
        assert!(view.refused.is_none());
        let step = view.step().into_string();
        assert!(step.contains("Refreshing Peloton's classes"), "{step}");
        assert!(step.contains("…"), "{step}");
    }

    /// A page drawn from the store says which step the run is on, and
    /// nothing once the run has ended.
    #[test]
    fn the_step_is_shown_while_going_and_not_after() {
        let view = watch::Sender::new(View {
            going: true,
            ..View::default()
        });
        Following(&view).started(&Step::RefreshingClasses);
        let going = view.borrow().step().into_string();
        assert!(going.contains("Refreshing Peloton's classes"), "{going}");

        view.send_modify(|view| {
            view.going = false;
            view.ended += 1;
        });
        assert_eq!(view.borrow().step().into_string(), "");
    }

    /// **A page redraws once for each run that ends after it was drawn**,
    /// including one it was never sent going, and not for one before it.
    #[test]
    fn a_run_that_ended_is_sent_as_ended_once() {
        let mut since = 1;
        let ended_before = View {
            ended: 1,
            ..View::default()
        };
        assert_eq!(step_event(&mut since, &ended_before).0, "step");

        let ended_since = View {
            ended: 2,
            ..View::default()
        };
        assert_eq!(step_event(&mut since, &ended_since).0, "ended");
        assert_eq!(step_event(&mut since, &ended_since).0, "step");
    }

    /// **A run refused by the terminal's lock redraws nothing**: it never
    /// happened, so the count of runs ended does not move.
    #[test]
    fn a_run_refused_by_the_lock_is_not_an_ending() {
        let before = View {
            ended: 3,
            ..View::default()
        };
        let refused = View {
            refused: Some("a run of next is already going".to_owned()),
            ..before
        };
        let mut since = 3;
        let (name, shown) = step_event(&mut since, &refused);
        assert_eq!(name, "step");
        assert!(
            shown
                .into_string()
                .contains("a run of next is already going")
        );
    }
}
