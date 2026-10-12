//! `fitness next` — collect everything, say where the plan stands, and deliver
//! what is next.
//!
//! **The use case is `application::next`** (#397), so a browser can offer the
//! same run. What is left here is the transport's: reading the clock and the
//! flags, composing the use case from this build's adapters, printing progress
//! as each step runs, and rendering the report.

use std::{
    io::Write as _,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use application::{
    commit::{CommitPorts, Committing},
    microcycle::{Microcycle, MicrocyclePorts},
    next::{
        CyclingPorts, Done, GymPorts, Next, NextError, NextPorts, NoAccess, Prescribed, Progress,
        Step, Undelivered, WhatIsNext,
    },
};
use domain::{
    normalised::OperatorZone,
    schedule::{DayPart, Discipline, PartOfDay},
};
use infrastructure::{
    SqliteCanonicalGymSessionStore, SqliteCyclingDeliveryStore, SqliteCyclingSessionLog,
    SqliteDiaryStore, SqliteExerciseHistory, SqliteGenerationParameterStore,
    SqliteGymMesocycleStore, SqliteNormalisedGymSessionReader, SqlitePelotonClassStore,
    SqlitePlanStore, SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore,
    SqliteRiddenVenues, connect,
    peloton::{
        PelotonClassSource, PelotonHoldingRides, PelotonProgrammes, PelotonRideDestination,
        catalogue::ClassCatalogue,
    },
};

use crate::{
    Failure, catalogue, config, cycling, exit, holidays, output, plan, prescribing, wiring,
};

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
    credentials: &infrastructure::Credentials,
) -> Result<(), Failure> {
    let now = moment(zone, date, part)?;
    let pool = connect(database).await?;

    // **This build collects the gym from Hevy and cycling from Peloton**, and
    // the catalogue says so: a second source for either is an entry there and
    // an arm in `wiring`.
    let no_access = |failure: Failure| NoAccess {
        detail: failure.message_text().to_owned(),
    };
    let access = |discipline| -> Result<_, Failure> {
        Ok(
            crate::source_access(known(discipline)?.collects(), None, credentials)
                .map_err(no_access),
        )
    };
    let gym_records =
        wiring::hevy_records(access(Discipline::Gym)?, database, zone.clone()).await?;
    let cycling_records =
        wiring::peloton_records(access(Discipline::Cycling)?, database, zone.clone()).await?;

    // **A credential that is absent costs what needs it, and nothing else.**
    // The catalogue answers choosing a class and building a mesocycle without
    // Peloton; refreshing it and delivering to the stack are what it costs.
    let peloton = plan::peloton(credentials);
    let stored = SqlitePelotonClassStore::new(pool.clone());
    let classes = peloton.as_ref().ok().map(|(classes, _)| classes);
    let catalogue = ClassCatalogue::over(&stored, classes);
    let unreachable = |failure: &Failure| NoAccess {
        detail: failure.message_text().to_owned(),
    };
    let class_source = peloton
        .as_ref()
        .map(|(classes, _)| PelotonClassSource::new(classes, &stored))
        .map_err(unreachable);
    let ride_destination = peloton
        .as_ref()
        .map(|(classes, stack)| PelotonRideDestination::new(classes, stack))
        .map_err(unreachable);
    let gym_destination =
        prescribing::hevy_routines(known(Discipline::Gym)?.delivers_to(), credentials)
            .map_err(no_access);

    let plans = SqlitePlanStore::new(pool.clone(), zone.clone());
    let parameters = SqliteGenerationParameterStore::new(pool.clone());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    let next = Next::new(NextPorts {
        gym_records,
        cycling_records,
        normalised: SqliteNormalisedGymSessionReader::new(pool.clone()),
        canonical: SqliteCanonicalGymSessionStore::new(pool.clone()),
        classes: class_source,
        holidays: holidays::Calendars,
        diary: SqliteDiaryStore::new(pool.clone()),
        plans: plans.clone(),
        microcycle: Microcycle::new(
            MicrocyclePorts {
                diary: SqliteDiaryStore::new(pool.clone()),
                plans: plans.clone(),
                gym_deliveries: deliveries.clone(),
                cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
                gym_performed: SqliteCanonicalGymSessionStore::new(pool.clone()),
                cycling_performed: SqliteCyclingSessionLog::new(pool.clone()),
            },
            destination(Discipline::Gym)?,
            destination(Discipline::Cycling)?,
        ),
        committing: Committing::new(
            CommitPorts {
                plans: plans.clone(),
                parameters: parameters.clone(),
                holding: PelotonHoldingRides::new(catalogue),
                ridden: SqliteRiddenVenues::new(pool.clone()),
                published: PelotonProgrammes::new(catalogue),
            },
            zone.clone(),
        ),
        gym: GymPorts {
            history: SqliteExerciseHistory::new(pool.clone()),
            mesocycles: SqliteGymMesocycleStore::new(pool.clone(), zone.clone()),
            plans,
            parameters,
            prescriptions: SqlitePrescribedWorkoutStore::new(pool.clone(), zone.id().to_owned()),
            lifecycle: deliveries.clone(),
            deliveries,
            destination: gym_destination,
        },
        cycling: CyclingPorts {
            riding: cycling::riding(&pool, zone, catalogue)?,
            destination: ride_destination,
        },
    });

    let report = next
        .run(now, skip, &Terminal::default())
        .await
        .map_err(refused)?;
    println!();
    output::plan_standing(&report.plan);
    println!();
    render(&report.next)
}

/// Why the run could not answer at all, as the operator is told it.
fn refused(error: NextError) -> Failure {
    match error {
        NextError::NoWeek => Failure::message(
            format!("{error}. Record one first: fitness schedule add"),
            exit::USAGE,
        ),
        NextError::Store(error) => Failure::from(error),
        NextError::Normalisation(error) => Failure::from(error),
        NextError::Skip(_) => Failure::usage(&error),
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
#[derive(Default)]
struct Terminal {
    /// Whether a step's name is on the current line, waiting for its outcome.
    open: AtomicBool,
}

impl Progress for Terminal {
    fn started(&self, step: &Step) {
        print!("{}", output::step(step));
        let _ = std::io::stdout().flush();
        self.open.store(true, Ordering::Relaxed);
    }

    fn estimated(&self, _step: &Step, outstanding: u64, takes: std::time::Duration) {
        if output::worth_announcing(takes) {
            println!(
                "{outstanding} classes to read, about {}",
                output::roughly(takes)
            );
            self.open.store(false, Ordering::Relaxed);
        }
    }

    fn finished(&self, done: &Done) {
        if !self.open.swap(false, Ordering::Relaxed) {
            print!("{}", output::step(&done.step));
        }
        println!("{}", output::outcome(done));
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

/// Where a discipline's sessions are put, as the catalogue names it.
pub fn destination(discipline: Discipline) -> Result<application::DestinationName, Failure> {
    let known = known(discipline)?;
    application::DestinationName::try_from(known.delivers_to().name().to_owned())
        .map_err(|error| Failure::usage(&error))
}

fn known(discipline: Discipline) -> Result<&'static catalogue::KnownDiscipline, Failure> {
    catalogue::discipline(discipline.as_str()).ok_or_else(|| {
        Failure::message(
            format!("this build has no daily loop for {discipline}"),
            exit::USAGE,
        )
    })
}
