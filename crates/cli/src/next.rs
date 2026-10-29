//! `fitness next` — collect everything, say where the plan stands, and deliver
//! what is next.
//!
//! **The use case is `application::next`** (#397), so a browser can offer the
//! same run, and the configurator composes it from this build's adapters
//! (#398). What is left here is the transport's: reading the clock and the
//! flags, printing progress as each step runs, and rendering the report.

use std::{io::Write as _, path::Path, sync::Mutex};

use application::next::{Done, NextError, Prescribed, Progress, Step, Undelivered, WhatIsNext};
use configurator::{next::NextFailure, telemetry::Frontend};
use domain::{
    normalised::OperatorZone,
    schedule::{DayPart, PartOfDay},
};

use crate::{Failure, config, cycling, exit, output};

/// Collect everything, report where the plan stands, and deliver what is next.
///
/// # Errors
///
/// [`Failure`] if the store is unavailable, if no week has been recorded, if a
/// skip cannot be taken, or if the next session could not be prescribed or
/// delivered. A source that cannot be reached while collecting is a failed step
/// and nothing more (§ 36): the record already landed still answers.
pub async fn next(
    database: &Path,
    zone: &OperatorZone,
    date: Option<&str>,
    part: Option<&str>,
    skip: Option<&str>,
    credentials: &configurator::Credentials,
) -> Result<(), Failure> {
    let now = moment(zone, date, part)?;
    // **Boxed, because the run is large**: it holds every source's walk, and a
    // third (#409) took it past what a stack frame should carry.
    let report = Box::pin(configurator::next::run(
        database,
        zone,
        credentials,
        now,
        skip,
        Frontend::Cli,
        &Terminal::default(),
    ))
    .await
    .map_err(refused)?;
    println!();
    output::plan_standing(&report.plan);
    println!();
    render(&report.next)
}

/// Why the run could not answer at all, as the operator is told it.
fn refused(failure: NextFailure) -> Failure {
    match failure {
        NextFailure::Next(error @ NextError::NoWeek) => Failure::message(
            format!("{error}. Record one first: fitness schedule add"),
            exit::USAGE,
        ),
        NextFailure::Next(error @ NextError::AlreadyRunning) => {
            Failure::message(error.to_string(), exit::ALREADY_RUNNING)
        }
        NextFailure::Next(NextError::Store(error)) | NextFailure::Store(error) => {
            Failure::from(error)
        }
        NextFailure::Next(error @ NextError::Skip(_)) => Failure::usage(&error),
        other => Failure::from(other),
    }
}

/// What is next, and the failure that should set the exit code, if any.
///
/// **A session that could not be prescribed or delivered fails the run**, after
/// it has been reported: the operator reading the terminal has the reason
/// already, and the exit code is for whatever started the run. An absent
/// credential is not a failure (§ 36) — it is said, and the run succeeds.
fn render(next: &WhatIsNext) -> Result<(), Failure> {
    let (session, prescribed) = match next {
        WhatIsNext::Session {
            session,
            prescribed,
        } => (session, prescribed),
        WhatIsNext::MicrocycleComplete { after } => {
            output::microcycle_complete(*after);
            return Ok(());
        }
        WhatIsNext::NoPlanCovers { week_of } => {
            output::no_plan_covers(*week_of);
            return Ok(());
        }
        WhatIsNext::NoSlot { from } => {
            output::no_next_slot(*from);
            return Ok(());
        }
    };
    output::next_session(session);

    match prescribed {
        Prescribed::Gym(Err(error)) => {
            output::not_prescribed(error);
            Err(Failure::message(error.to_string(), exit::USAGE))
        }
        Prescribed::Gym(Ok(application::next::GymPrescribed::NothingPlanned { from, last })) => {
            output::nothing_planned(*from, last.as_ref());
            Ok(())
        }
        Prescribed::Gym(Ok(application::next::GymPrescribed::Issued { issued, delivery })) => {
            output::prescribed_session(issued);
            println!();
            match delivery {
                Ok(delivered) => {
                    output::delivery(delivered);
                    Ok(())
                }
                Err(Undelivered::NoAccess(why)) => {
                    output::not_delivered(&why.detail);
                    Ok(())
                }
                Err(Undelivered::Failed(error)) => {
                    output::not_delivered(&error.to_string());
                    Err(Failure::message(error.to_string(), exit::SOURCE))
                }
            }
        }
        Prescribed::Cycling(Err(error)) => {
            output::not_prescribed(error);
            Err(cycling::riding_failure(session.slot.date, error))
        }
        Prescribed::Cycling(Ok(prescribed)) => {
            println!("  {}", cycling::heading(&prescribed.ride));
            println!();
            cycling::session(&prescribed.ride);
            println!();
            match &prescribed.delivery {
                Ok(placed) => {
                    cycling::delivered(&prescribed.ride, placed);
                    Ok(())
                }
                Err(Undelivered::NoAccess(why)) => {
                    output::not_delivered(&why.detail);
                    Ok(())
                }
                Err(Undelivered::Failed(error)) => {
                    output::not_delivered(&error.to_string());
                    Err(Failure::message(error.to_string(), exit::SOURCE))
                }
            }
        }
    }
}

/// Progress, a line per step, printed as each one runs.
///
/// **The step's name is printed when it starts**, so a long first walk or the
/// first catalogue refresh says what it is doing rather than looking hung, and
/// its outcome finishes the line.
/// One line per step, its outcome beside its name.
///
/// **Steps run at once** (#468), and a line can only wait for one outcome. So
/// the first step to start has its name printed and waits on it; a step that
/// starts while it waits is printed whole when it ends, and one that ends
/// first is held until the waiting line is finished.
#[derive(Default)]
struct Terminal {
    line: Mutex<Line>,
}

#[derive(Default)]
struct Line {
    /// The step whose name is on the current line, waiting for its outcome.
    open: Option<Step>,
    /// Steps that ended while another's line was open, in the order they did.
    held: Vec<String>,
}

impl Terminal {
    fn line(&self) -> std::sync::MutexGuard<'_, Line> {
        self.line
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Progress for Terminal {
    fn started(&self, step: &Step) {
        let mut line = self.line();
        if line.open.is_none() {
            print!("{}", output::step(step));
            let _ = std::io::stdout().flush();
            line.open = Some(step.clone());
        }
    }

    fn estimated(&self, step: &Step, outstanding: u64, takes: std::time::Duration) {
        let mut line = self.line();
        if output::worth_announcing(takes) && line.open.as_ref() == Some(step) {
            println!(
                "{outstanding} classes to read, about {}",
                output::roughly(takes)
            );
            line.open = None;
            for held in std::mem::take(&mut line.held) {
                println!("{held}");
            }
        }
    }

    fn finished(&self, done: &Done) {
        let printed = self.line().finished(done);
        for text in printed {
            println!("{text}");
        }
    }
}

impl Line {
    /// What to print now that `done` has ended: its outcome and whatever was
    /// held behind it, or the whole of it, or nothing yet.
    fn finished(&mut self, done: &Done) -> Vec<String> {
        if self.open.as_ref() == Some(&done.step) {
            self.open = None;
            let mut printed = vec![output::outcome(done)];
            printed.append(&mut self.held);
            return printed;
        }
        let whole = format!("{}{}", output::step(&done.step), output::outcome(done));
        if self.open.is_some() {
            self.held.push(whole);
            Vec::new()
        } else {
            vec![whole]
        }
    }
}

/// Where the operator's day has got to.
///
/// **A part as well as a date**, because how much of a session's window is
/// left is the question every state turns on (#185). Both default to the
/// clock in the operator's own zone; `--date` moves the day and `--part` the
/// part, so a run can be asked what a Saturday evening would have said.
fn moment(zone: &OperatorZone, date: Option<&str>, part: Option<&str>) -> Result<DayPart, Failure> {
    let here = jiff::Timestamp::now().to_zoned(zone.as_time_zone());
    let date = match date {
        Some(text) => config::named_date(text).map_err(|error| Failure::usage(&error))?,
        None => here.date(),
    };
    let part = match part {
        Some(text) => {
            PartOfDay::try_from(text.to_owned()).map_err(|error| Failure::usage(&error))?
        }
        None => DayPart::containing(here.datetime()).part,
    };
    Ok(DayPart::new(date, part))
}
