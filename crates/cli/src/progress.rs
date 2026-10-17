//! Saying that a long collection is still alive (#437).
//!
//! A first collection can be quiet for a long time while it waits on a
//! source — sixteen minutes, on 2026-10-14, before `extract garmin.activities`
//! printed anything after its first line. What the operator needs then is to
//! know the run has not crashed: a spinner naming the walk under way while it
//! runs, and each walk's outcome as soon as it ends rather than all of them at
//! the end. No count and no estimate: Garmin's activity list does not say how
//! long it is, so nothing honest could be shown for every walk.
//!
//! **Only on a terminal.** The spinner redraws one line on standard error, which
//! in a log or a pipe would be noise, so anything that is not a terminal gets the
//! outcome lines and nothing else.

use std::{
    io::{IsTerminal, Write},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    thread::{self, JoinHandle},
    time::Duration,
};

use configurator::wiring::{Walk, WalkReporter};
use domain::landing::LandingStream;

use crate::output;

/// How often the spinner turns.
const TICK: Duration = Duration::from_millis(120);

const FRAMES: [&str; 4] = ["-", "\\", "|", "/"];

/// Erases the current line of a terminal and returns to its start.
const ERASE: &str = "\r\x1b[2K";

/// What the spinner and the outcome lines share. One lock, so that an outcome
/// line is never printed into the middle of a half-drawn spinner.
struct State {
    label: String,
    /// Whether the spinner's line is on screen and has to be erased before
    /// anything else is printed.
    drawn: bool,
    stopped: bool,
}

/// A spinner for one extract, and the outcome of each walk behind it as it ends.
pub struct Progress {
    state: Arc<Mutex<State>>,
    ticker: Option<JoinHandle<()>>,
}

fn locked(state: &Mutex<State>) -> MutexGuard<'_, State> {
    // A poisoned lock means a print panicked, and panics are forbidden; the
    // state is still a label and two flags, so carrying on is right.
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Progress {
    /// Start the spinner, labelled with the stream the extract was asked for.
    pub fn start(stream: &LandingStream) -> Self {
        let state = Arc::new(Mutex::new(State {
            label: stream.to_string(),
            drawn: false,
            stopped: false,
        }));
        let ticker = if std::io::stderr().is_terminal() {
            let shared = Arc::clone(&state);
            // A thread that could not be started costs the spinner, not the run.
            thread::Builder::new()
                .name("progress".to_owned())
                .spawn(move || spin(&shared))
                .ok()
        } else {
            None
        };
        Self { state, ticker }
    }

    /// Print something with the spinner out of the way.
    fn say(&self, print: impl FnOnce()) {
        let mut state = locked(&self.state);
        if state.drawn {
            let mut stderr = std::io::stderr();
            let _ = write!(stderr, "{ERASE}");
            let _ = stderr.flush();
            state.drawn = false;
        }
        print();
        let _ = std::io::stdout().flush();
        // Held until the line is out, so the spinner cannot redraw over it.
        drop(state);
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        locked(&self.state).stopped = true;
        if let Some(ticker) = self.ticker.take() {
            ticker.thread().unpark();
            let _ = ticker.join();
        }
        self.say(|| {});
    }
}

impl WalkReporter for Progress {
    fn started(&self, stream: &LandingStream) {
        locked(&self.state).label = stream.to_string();
    }

    fn finished(&self, walk: &Walk) {
        self.say(|| match &walk.result {
            Ok(summary) => output::walk_succeeded(&walk.stream, summary),
            Err(error) => output::not_collected(&walk.stream, &error.to_string()),
        });
    }
}

fn spin(state: &Mutex<State>) {
    for frame in FRAMES.iter().cycle() {
        {
            let mut state = locked(state);
            if state.stopped {
                return;
            }
            let mut stderr = std::io::stderr();
            let _ = write!(stderr, "{ERASE}{frame} {} …", state.label);
            let _ = stderr.flush();
            state.drawn = true;
        }
        thread::park_timeout(TICK);
    }
}
