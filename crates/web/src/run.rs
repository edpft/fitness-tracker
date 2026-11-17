//! The run of `next` this server holds, and what every open page is shown of it.
//!
//! **The run belongs to the server, not to the request that started it.** A
//! page that is closed, or a phone that sleeps, costs the run nothing; a page
//! opened mid-run is shown where it has got to and then each step as it ends.
//! Opening a page is what starts one (#466), so nobody presses anything.
//!
//! **A page collects only what its charts read** (#559): the front page runs
//! the whole of `next`, and a chart page only its own streams, so runs over
//! different streams go at the same time. A run already covering what a page
//! asks for is joined, here if this server started it and through the store's
//! locks if the terminal did.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
};

use application::Part;
use application::next::NextError;
use application::next::{Done, Progress, Scope, Step};
use configurator::{
    next::NextFailure,
    telemetry::{Frontend, Run},
};
use domain::{normalised::OperatorZone, schedule::DayPart};
use maud::{Markup, html};
use tokio::sync::watch;

use crate::{collected, report};

/// What the operator configured, which every run reads.
pub struct Setting {
    pub store: configurator::store::Store,
    pub zone: OperatorZone,
    /// Where the credentials are kept, `None` where nothing says.
    ///
    /// **A place, not what was in it** (#438): each run reads the file as it
    /// stands, so a credential stored while the server runs is the one the
    /// next run uses.
    pub credentials: Option<PathBuf>,
    /// The build serving, which a kept drawing is keyed on (#805): `None`
    /// where it is not known, and every page is drawn as it is opened.
    pub build: Option<&'static str>,
}

/// The latest run, as the page shows it.
#[derive(Clone, Default)]
pub struct View {
    /// Why the latest attempt did not start or could not answer, above the
    /// run before it.
    refused: Option<String>,
    steps: Vec<Line>,
    going: bool,
    /// How many runs have ended since the server started, so a page drawn
    /// from the store knows whether a run has ended since it was drawn.
    ///
    /// **A count rather than a flag**, because a page is sent the latest view
    /// and not every view: a run that starts and ends between two of them
    /// would leave a flag where it found it.
    ended: u64,
    /// The parts this server's runs have collected since none was going, in
    /// order (#830).
    collected: Vec<Part>,
    /// How many parts runs before those collected, so a page knows which it
    /// has been told of: a count, as `ended` is.
    collected_before: u64,
}

/// One step: named as it starts, with its outcome once it ends.
#[derive(Clone)]
struct Line {
    step: Step,
    done: Option<Done>,
}

impl View {
    /// Whether what each page reads has been collected (#666), under why
    /// the latest run did not start or answer, if it did not.
    ///
    /// **Not the steps**: the operator, 2026-11-09, *"I just want to know, for
    /// the high level metrics and prescriptions has the data been fetched and
    /// normalised."* What each step found is in telemetry.
    pub fn render(&self) -> Markup {
        let steps = self
            .steps
            .iter()
            .map(|line| (&line.step, line.done.as_ref().map(|done| &done.outcome)));
        html! {
            @if let Some(refused) = &self.refused { section.card { p.failed { (refused) } } }
            (collected::matrix(&steps, self.going))
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

    /// How many parts runs have collected since the server started.
    pub const fn collected(&self) -> u64 {
        self.collected_before + self.collected.len() as u64
    }

    /// The parts collected after the first `since`.
    fn collected_since(&self, since: u64) -> &[Part] {
        let skip =
            usize::try_from(since.saturating_sub(self.collected_before)).unwrap_or(usize::MAX);
        self.collected.get(skip..).unwrap_or_default()
    }
}

/// Whether a card's data is being collected, and by whom (#830).
#[derive(Debug, PartialEq, Eq)]
pub enum Waiting {
    /// Nothing is collecting what it reads.
    No,
    /// This server's runs are, and these parts of it are still to come: each
    /// is sent to every open page as it is collected.
    Here(Vec<Part>),
    /// A run started from the terminal is, which sends this server nothing.
    Elsewhere,
}

/// The name of the event a page is sent as `part` is collected.
pub fn event(part: &Part) -> String {
    match part {
        Part::Fetching(stream) => format!("fetched-{stream}"),
        Part::Normalising(stream) => format!("normalised-{stream}"),
        Part::Canonicalising(layer) => format!("canonical-{}", layer.as_str()),
        Part::FetchingCalendar(calendar) => format!("calendar-{}", calendar.as_str()),
        Part::Delivering => "delivered".to_owned(),
    }
}

/// What a page asks a run to collect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// The whole of `next`, as the front page does: the operator, 2026-10-23,
    /// *"Opening the main page should trigger a collection run for all
    /// metrics."*
    Everything,
    /// Only what a chart reads, delivering nothing.
    Only(Scope),
}

impl Asked {
    /// Whether a run asked for `self` collects everything `other` asks for.
    fn covers(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Everything, _) => true,
            (Self::Only(_), Self::Everything) => false,
            (Self::Only(going), Self::Only(asked)) => going.covers(asked),
        }
    }
}

/// The runs this server holds.
pub struct Runs {
    setting: Setting,
    /// What is derived from the store, held while nothing it reads moves
    /// (#830).
    held: configurator::held::Held,
    /// Each card's last drawing (#830).
    drawings: crate::card::Drawings,
    view: watch::Sender<View>,
    going: Mutex<Vec<Asked>>,
}

impl Runs {
    pub fn new(setting: Setting) -> Self {
        Self {
            setting,
            held: configurator::held::Held::default(),
            drawings: crate::card::Drawings::default(),
            view: watch::Sender::new(View::default()),
            going: Mutex::new(Vec::new()),
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

    /// What is held in memory.
    pub const fn held(&self) -> &configurator::held::Held {
        &self.held
    }

    /// Where the plan stands and what is next, as held, for a run or the
    /// question whether one is wanted (#830).
    const fn held_next(&self) -> configurator::held::HeldNext<'_> {
        let setting = &self.setting;
        configurator::held::HeldNext::new(&self.held, &setting.store, &setting.zone, setting.build)
    }

    /// Each card's last drawing.
    pub const fn drawings(&self) -> &crate::card::Drawings {
        &self.drawings
    }

    pub fn view(&self) -> View {
        self.view.borrow().clone()
    }

    /// Whether a run is collecting anything `scope` reads, so a page reading
    /// the store now could find a layer emptied and not yet rebuilt (#467).
    ///
    /// **This server's runs and then the store's locks**: a run this server
    /// has just started is going before its thread takes its parts, and one
    /// the terminal started is known only by them. A lock that cannot be
    /// tried says nothing is collecting, and the page reads the store as it
    /// did before.
    pub fn collecting(&self, scope: &Scope) -> bool {
        let here = self
            .going
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|run| match run {
                Asked::Everything => true,
                Asked::Only(going) => !going.parts().is_disjoint(scope.parts()),
            });
        here || configurator::next::collecting(&self.setting.store, scope).unwrap_or(false)
    }

    /// Whether a run is collecting anything `scope` reads, as a card waits on
    /// it (#830).
    ///
    /// **This server's runs part by part**, so a card is drawn as soon as
    /// what it reads is collected, while the run collects the rest. **The
    /// store's locks for the parts none of them covers**, since a run this
    /// server started holds them for the whole of it.
    pub fn waiting(&self, scope: &Scope) -> Waiting {
        let (here, elsewhere): (Vec<&Part>, Vec<&Part>) = {
            let going = self.going.lock().unwrap_or_else(PoisonError::into_inner);
            scope.parts().iter().partition(|part| {
                going.iter().any(|run| match run {
                    Asked::Everything => true,
                    Asked::Only(going) => going.has(part),
                })
            })
        };
        let view = self.view.borrow();
        let still: Vec<Part> = here
            .into_iter()
            .filter(|part| !view.collected.contains(part))
            .cloned()
            .collect();
        if !still.is_empty() {
            return Waiting::Here(still);
        }
        let elsewhere = Scope::new(elsewhere.into_iter().cloned());
        if !elsewhere.parts().is_empty()
            && configurator::next::collecting(&self.setting.store, &elsewhere).unwrap_or(false)
        {
            Waiting::Elsewhere
        } else {
            Waiting::No
        }
    }

    /// `parts` collected, as though this server's run had collected them.
    #[cfg(test)]
    pub fn collect(&self, parts: &[Part]) {
        Following(&self.view).collected(parts);
    }

    /// A run going over `asked`, as though this server had started one.
    #[cfg(test)]
    pub fn going_over(&self, asked: &Asked) {
        let _going = self.going(asked);
    }

    /// The run going over `asked` ended, as though this server's had.
    #[cfg(test)]
    pub fn ended_over(&self, asked: &Asked) {
        self.end(asked);
    }

    /// Start a run of what a page asks for, as [`Self::start`] does, unless
    /// nothing it reads could hold anything new (#612).
    ///
    /// **Only a chart's page asks**: the front page delivers, and its report
    /// is what it shows. A run going that covers the page is joined whatever
    /// the store says, and a store that cannot say starts the run as before.
    pub async fn open(self: &Arc<Self>, asked: &Asked) {
        if let Asked::Only(scope) = asked
            && !self.covered(asked)
            && !Box::pin(self.wanted(scope)).await
        {
            configurator::telemetry::not_due();
            return;
        }
        self.start(asked);
    }

    /// Whether this server has a run going that covers `asked`.
    fn covered(&self, asked: &Asked) -> bool {
        self.going
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|run| run.covers(asked))
    }

    /// Whether a run over `scope` would fetch or derive anything now.
    async fn wanted(&self, scope: &Scope) -> bool {
        let setting = &self.setting;
        let Ok(credentials) = configurator::config::credentials_at(setting.credentials.as_deref())
        else {
            return true;
        };
        let here = jiff::Timestamp::now().to_zoned(setting.zone.as_time_zone());
        Box::pin(configurator::next::wanted(
            &self.held_next(),
            &credentials,
            DayPart::containing(here.datetime()),
            scope,
            Frontend::Web,
        ))
        .await
        .unwrap_or(true)
    }

    /// Start a run of what a page asks for, unless this server has one going
    /// that covers it, in which case the page opening joins that.
    ///
    /// **A run that covers only part of it is the run's to join**, as one
    /// going in the terminal is: it waits on the store's locks for the parts
    /// another run holds and collects the rest itself. So is a run that could
    /// not be started at all: there is no one request to answer, so it is
    /// said where every page will see it.
    pub fn start(self: &Arc<Self>, asked: &Asked) {
        if !self.going(asked) {
            configurator::telemetry::joined();
            return;
        }

        // Made here, while the page load that asked for it is going, so the
        // run is recorded as its child (#628).
        let run = match asked {
            Asked::Everything => Run::everything(Frontend::Web),
            Asked::Only(scope) => Run::over(Frontend::Web, scope),
        };
        let runs = Arc::clone(self);
        let started = asked.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("next".to_owned())
            .spawn(move || runs.run(&started, run))
        {
            self.refuse(&format!("the run could not be started: {error}"));
            self.end(asked);
        }
    }

    /// `asked` going, unless a run going covers it: whether it is new.
    ///
    /// **The steps shown are this server's runs since none was going**, so a
    /// run started beside another adds to what the page shows.
    fn going(&self, asked: &Asked) -> bool {
        let mut going = self.going.lock().unwrap_or_else(PoisonError::into_inner);
        if going.iter().any(|run| run.covers(asked)) {
            return false;
        }
        if going.is_empty() {
            let (ended, collected_before) = {
                let view = self.view.borrow();
                (view.ended, view.collected())
            };
            self.view.send_replace(View {
                going: true,
                ended,
                collected_before,
                ..View::default()
            });
        }
        going.push(asked.clone());
        true
    }

    /// Say why a page's run could not be asked for, where every page sees it.
    pub fn refuse(&self, message: &str) {
        self.view
            .send_modify(|view| view.refused = Some(message.to_owned()));
    }

    /// The run, on a thread and a runtime of its own.
    ///
    /// **Its own runtime**, so nothing about the run's futures has to suit the
    /// server's: what is shared is the channel each step is sent down.
    ///
    /// **Only a run of the whole of `next` delivers**, and what it delivers is
    /// what the home page's Next card is drawn from (#830): handed over where
    /// it is held at what the store then says, and held once derived.
    fn run(&self, asked: &Asked, run: Run) {
        let setting = &self.setting;
        let started = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| {
                configurator::config::credentials_at(setting.credentials.as_deref())
                    .map(|credentials| (runtime, credentials))
                    .map_err(|error| error.to_string())
            });
        let (runtime, credentials) = match started {
            Ok(started) => started,
            Err(error) => {
                self.refuse(&format!("the run could not be started: {error}"));
                return self.end(asked);
            }
        };
        let here = jiff::Timestamp::now().to_zoned(setting.zone.as_time_zone());
        let following = Following(&self.view);
        let held = self.held_next();
        let failed = match asked {
            Asked::Everything => {
                match runtime.block_on(configurator::next::run(
                    &held,
                    &credentials,
                    DayPart::containing(here.datetime()),
                    None,
                    run,
                    &following,
                )) {
                    // **Joining the terminal's run is not a failure**: the
                    // cards follow it through the store's locks.
                    Ok(_) | Err(NextFailure::Next(NextError::Joined)) => None,
                    Err(failure) => Some(unanswered(&failure)),
                }
            }
            Asked::Only(scope) => runtime
                .block_on(configurator::next::collect(
                    &held,
                    &credentials,
                    DayPart::containing(here.datetime()),
                    scope,
                    run,
                    &following,
                ))
                .err()
                .map(|failure| failure.to_string()),
        };
        if let Some(failure) = failed {
            self.refuse(&failure);
        }
        self.end(asked);
    }

    /// A run is over, and every page is told.
    fn end(&self, asked: &Asked) {
        let still = {
            let mut going = self.going.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(at) = going.iter().position(|run| run == asked) {
                going.remove(at);
            }
            !going.is_empty()
        };
        self.view.send_modify(|view| {
            view.going = still;
            view.ended += 1;
        });
    }
}

/// What a page drawn from the store is sent of a change to the run: an event
/// for each part collected since `collected`, then the step as
/// [`step_event`] names it.
///
/// **No event is sent with no data**: a browser drops one unseen (the
/// `EventSource` processing model dispatches nothing for an empty data
/// buffer), so every card waiting on a part, and on a run ending with no
/// step to show, waited for ever on 2026-11-17 while `curl` showed every
/// event (#830). A part's event carries its own name, and the step is sent
/// inside an element of its own, which is empty once the run has ended.
pub fn page_events(since: &mut u64, collected: &mut u64, view: &View) -> Vec<(String, String)> {
    let mut events: Vec<(String, String)> = view
        .collected_since(*collected)
        .iter()
        .map(|part| {
            let name = event(part);
            (name.clone(), name)
        })
        .collect();
    *collected = view.collected();
    let (name, shown) = step_event(since, view);
    events.push((name.to_owned(), html! { div { (shown) } }.into_string()));
    events
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

/// Progress, sent to every open page as each step starts and ends.
struct Following<'a>(&'a watch::Sender<View>);

impl Progress for Following<'_> {
    fn started(&self, step: &Step) {
        self.0.send_modify(|view| {
            view.steps.push(Line {
                step: step.clone(),
                done: None,
            });
        });
    }

    /// **Not shown**: the home page shows no step (#666), and a chart's page
    /// only which one the run is on.
    fn estimated(&self, _step: &Step, _outstanding: u64, _takes: std::time::Duration) {}

    fn collected(&self, parts: &[Part]) {
        if !parts.is_empty() {
            self.0
                .send_modify(|view| view.collected.extend(parts.iter().cloned()));
        }
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
                    done: Some(done.clone()),
                }),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application::{
        Part,
        next::{Done, Found, NoAccess, Progress as _, Scope, Step, StepFailure},
    };
    use domain::{landing::LandingStream, normalised::OperatorZone};
    use tokio::sync::watch;

    use super::{Asked, Following, Runs, Setting, View, Waiting, page_events, step_event};

    fn only(streams: &[&str]) -> Asked {
        Asked::Only(Scope::new(streams.iter().map(|stream| {
            Part::Normalising(LandingStream::try_from(*stream).expect("a stream"))
        })))
    }

    fn shown(view: &watch::Sender<View>) -> String {
        view.borrow().render().into_string()
    }

    /// **A chart's page shows the step as it starts**, so a long one says
    /// what it is doing rather than looking hung, and its outcome replaces the
    /// ellipsis.
    #[test]
    fn a_step_is_shown_as_it_starts_and_finished_in_place() {
        let view = watch::Sender::new(View {
            going: true,
            ..View::default()
        });
        let following = Following(&view);

        following.started(&Step::RefreshingClasses);
        let started = view.borrow().step().into_string();
        assert!(
            started.contains("Refreshing Peloton's classes"),
            "{started}"
        );
        assert!(started.contains("…"), "{started}");

        following.finished(&Done {
            step: Step::RefreshingClasses,
            outcome: Ok(Found::Classes {
                added: 3,
                read: 412,
                outstanding: 0,
                stopped: None,
            }),
            took: std::time::Duration::from_secs(42),
        });
        let finished = view.borrow().step().into_string();
        assert!(
            finished.contains("done, 3 new classes (42.0 s)"),
            "{finished}"
        );
        assert!(!finished.contains("…"), "{finished}");
        assert_eq!(view.borrow().steps.len(), 1);
    }

    /// **The home page shows no step** (#666): a fetch that failed, reported
    /// only as it ended as a walk with no access is, is a red dot on the rows
    /// that read it, and its reason is not on the page.
    #[test]
    fn the_home_page_shows_a_failed_fetch_as_a_dot_and_not_its_reason() {
        let view = watch::Sender::new(View::default());
        let stream = LandingStream::try_from("hevy.workouts").expect("a stream");

        Following(&view).finished(&Done {
            step: Step::Fetching(stream),
            outcome: Err(StepFailure::NoAccess(NoAccess {
                detail: "no hevy credential".to_owned(),
            })),
            took: std::time::Duration::ZERO,
        });
        let shown = shown(&view);
        assert!(!shown.contains("Fetching hevy.workouts"), "{shown}");
        assert!(!shown.contains("no hevy credential"), "{shown}");
        let gym = shown
            .split(r#"<div class="row">"#)
            .find(|row| row.starts_with("<span>Gym sessions</span>"))
            .expect("the gym's row");
        assert!(gym.contains(r#"<i class="dot failed""#), "{gym}");
    }

    /// **A page opened while a run covering it is going joins it**: nothing
    /// starts, and the page is shown where the run has got to. A chart's page
    /// joins the front page's run.
    #[test]
    fn a_page_opened_mid_run_joins_it() {
        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let runs = Arc::new(Runs::new(Setting {
            store: runtime
                .block_on(configurator::store::Store::open(
                    &directory.path().join("fitness.db"),
                ))
                .expect("the store"),
            zone: OperatorZone::try_from("Europe/London").expect("a zone"),
            credentials: None,
            build: None,
        }));
        runs.going
            .lock()
            .expect("not poisoned")
            .push(Asked::Everything);
        runs.view.send_modify(|view| view.going = true);
        Following(&runs.view).started(&Step::RefreshingClasses);

        runs.start(&Asked::Everything);
        runs.start(&only(&["garmin.hrv"]));

        let view = runs.view();
        assert_eq!(view.steps.len(), 1);
        assert_eq!(runs.going.lock().expect("not poisoned").len(), 1);
        assert!(view.refused.is_none());
        let step = view.step().into_string();
        assert!(step.contains("Refreshing Peloton's classes"), "{step}");
        assert!(step.contains("…"), "{step}");
    }

    /// **A page's data is being collected by any run over a part it reads**
    /// (#467): the whole of `next`, or a chart's run sharing a stream with it,
    /// and not a chart's run over other streams.
    #[test]
    fn a_page_is_collecting_while_a_run_reads_a_part_of_it() {
        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let store = runtime
            .block_on(configurator::store::Store::open(
                &directory.path().join("fitness.db"),
            ))
            .expect("the store");
        let fresh = || {
            Runs::new(Setting {
                store: store.clone(),
                zone: OperatorZone::try_from("Europe/London").expect("a zone"),
                credentials: None,
                build: None,
            })
        };
        let Asked::Only(hrv) = only(&["garmin.hrv", "garmin.activities"]) else {
            panic!("a chart's scope");
        };

        let runs = fresh();
        assert!(!runs.collecting(&hrv), "nothing is going");
        runs.going_over(&only(&["withings.measurements"]));
        assert!(!runs.collecting(&hrv), "a run over other streams");
        runs.going_over(&only(&["garmin.hrv"]));
        assert!(runs.collecting(&hrv), "a run sharing a stream");

        let runs = fresh();
        runs.going_over(&Asked::Everything);
        assert!(runs.collecting(&hrv), "the whole of next");
    }

    /// **A card waits only on the parts still to be collected** (#830): while
    /// the whole of `next` goes, the weigh-ins stop waiting once they are
    /// normalised, and the watch's HRV still waits.
    #[test]
    fn a_card_waits_only_on_what_is_still_to_be_collected() {
        let directory = tempfile::TempDir::new().expect("a temporary directory");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let runs = Runs::new(Setting {
            store: runtime
                .block_on(configurator::store::Store::open(
                    &directory.path().join("fitness.db"),
                ))
                .expect("the store"),
            zone: OperatorZone::try_from("Europe/London").expect("a zone"),
            credentials: None,
            build: None,
        });
        let normalising =
            |name: &str| Part::Normalising(LandingStream::try_from(name).expect("a stream"));
        let Asked::Only(weight) = only(&["withings.measurements"]) else {
            panic!("a chart's scope");
        };
        let Asked::Only(hrv) = only(&["garmin.hrv"]) else {
            panic!("a chart's scope");
        };

        assert_eq!(runs.waiting(&weight), Waiting::No, "nothing is going");
        runs.going_over(&Asked::Everything);
        assert_eq!(
            runs.waiting(&weight),
            Waiting::Here(vec![normalising("withings.measurements")])
        );

        runs.collect(&[normalising("withings.measurements")]);
        assert_eq!(runs.waiting(&weight), Waiting::No);
        assert_eq!(
            runs.waiting(&hrv),
            Waiting::Here(vec![normalising("garmin.hrv")])
        );
    }

    /// **Every open page is sent each part once**, as it is collected,
    /// whichever change to the run it came with, and none it was drawn after.
    #[test]
    fn each_page_is_sent_each_part_collected_once() {
        let view = watch::Sender::new(View {
            going: true,
            collected_before: 3,
            ..View::default()
        });
        let withings =
            Part::Normalising(LandingStream::try_from("withings.measurements").expect("a stream"));
        let hrv = Part::Normalising(LandingStream::try_from("garmin.hrv").expect("a stream"));
        let named = |events: Vec<(String, String)>| -> Vec<String> {
            events.into_iter().map(|(name, _)| name).collect()
        };
        // One page drawn before anything was collected, one after the first.
        let (mut first, mut second) = ((0, 3), (0, 4));

        Following(&view).collected(std::slice::from_ref(&withings));
        Following(&view).collected(std::slice::from_ref(&hrv));
        assert_eq!(
            named(page_events(&mut first.0, &mut first.1, &view.borrow())),
            [
                "normalised-withings.measurements",
                "normalised-garmin.hrv",
                "step"
            ]
        );
        assert_eq!(
            named(page_events(&mut second.0, &mut second.1, &view.borrow())),
            ["normalised-garmin.hrv", "step"]
        );
        assert_eq!(
            named(page_events(&mut first.0, &mut first.1, &view.borrow())),
            ["step"]
        );
    }

    /// **A run covers what it collects and nothing more**: the whole of
    /// `next` covers every chart, a chart's run covers a chart reading less,
    /// and none covers the whole of `next` or a chart reading something else.
    #[test]
    fn a_run_covers_only_what_it_collects() {
        let hrv = only(&["garmin.hrv", "garmin.activities"]);
        assert!(Asked::Everything.covers(&hrv));
        assert!(hrv.covers(&only(&["garmin.hrv"])));
        assert!(!hrv.covers(&Asked::Everything));
        assert!(!hrv.covers(&only(&["withings.measurements"])));
    }

    /// **Every event a page is sent carries data** (#830), or a browser drops
    /// it: a part collected, and a run ended with no step left to show.
    #[test]
    fn every_event_a_page_is_sent_carries_data() {
        let view = watch::Sender::new(View {
            going: true,
            ..View::default()
        });
        Following(&view).collected(&[Part::Delivering]);
        view.send_modify(|view| {
            view.going = false;
            view.ended += 1;
        });
        let events = page_events(&mut 0, &mut 0, &view.borrow());
        assert_eq!(
            events
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["delivered", "ended"]
        );
        for (name, data) in &events {
            assert!(!data.is_empty(), "{name} has no data");
        }
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
