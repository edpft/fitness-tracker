//! `fitness next` — where the microcycle is up to, and the session to deliver.
//!
//! The operator, 2026-09-19: *"This command is all about what's next, based on
//! what has happened and what should happen."* (#137)
//!
//! **It reports from the first session of the microcycle, not from the slot
//! before today** (#185). His first run of the installed build checked only the
//! previous slot, so it collected the gym and not the bike and could not say
//! where the week was up to: *"We should be checking from the last performed
//! session, not the last prescribed session"*, and then *"Maybe we should
//! always start from the beginning of the microcycle."*
//!
//! **Every source is collected first, and unconditionally.** Deciding what to
//! collect from what the store already holds is how the bike went unasked: the
//! store said nothing was outstanding because nothing had been prescribed. So
//! each discipline's record is collected and derived before anything is
//! judged. An unreachable source is reported and stepped past (§ 36).
//!
//! **Then each session of the microcycle gets a state**, and the first one
//! still to be prescribed is the one delivered. Recording why a session was
//! missed is #178's, which is done; moving anything because of it is #177's.

use std::path::Path;

use application::DiaryStore as _;
use domain::{
    normalised::OperatorZone,
    planner::SessionState,
    schedule::{DayPart, Discipline, PartOfDay, ScheduledSlot},
};
use infrastructure::{SqliteDiaryStore, connect};

use crate::{
    Failure, catalogue, committing, config, cycling, exit, gym, holidays, output, plan,
    rescheduling, wiring, wiring::Command,
};

/// Collect everything, report the microcycle, and deliver what is next.
///
/// # Errors
///
/// [`Failure`] if the store is unavailable, if no week has been recorded, or
/// if the next discipline's own `next` fails. A source that cannot be reached
/// while collecting is reported and stepped past (§ 36): the record already
/// landed still answers, and the next session is still worth delivering.
pub async fn next(
    database: &Path,
    zone: &OperatorZone,
    date: Option<&str>,
    part: Option<&str>,
    credentials: &infrastructure::Credentials,
) -> Result<(), Failure> {
    let now = moment(zone, date, part)?;

    let pool = connect(database).await?;
    let diary = SqliteDiaryStore::new(pool.clone()).diary().await?;
    if diary.patterns().is_empty() {
        return Err(Failure::message(
            "no week has been recorded, so nothing says what is next. \
             Record one first: fitness schedule add",
            exit::USAGE,
        ));
    }
    pool.close().await;

    // 1. Every source, before anything is judged.
    for discipline in [Discipline::Gym, Discipline::Cycling] {
        collect(discipline, database, zone, credentials).await?;
    }
    println!();

    // 2. The next concurrent mesocycle, if the one before it has ended (#222).
    let pool = connect(database).await?;
    if let Some(due) = committing::due(&pool, zone, now).await? {
        match committing::commit(&pool, zone, &due, credentials).await {
            Ok(()) => output::committed(&due),
            Err(failure) => output::not_committed(&due, &failure.message),
        }
    }

    // 3. Where the plan stands: every week since it began, and this one.
    let standing = rescheduling::standing(&pool, zone, now).await?;
    let diary = SqliteDiaryStore::new(pool.clone()).diary().await?;
    pool.close().await;

    let holidays = holidays::read().await;
    output::macrocycle(
        now.date,
        holidays.as_ref().map_err(Failure::message_text),
        standing.mesocycle,
    );
    output::rescheduled(&standing.weeks, &standing.reruns);

    let sessions = &standing.sessions;
    if sessions.is_empty() {
        // **Two empty answers, and they are different facts.** A week the
        // diary holds nothing for is a week off; a week it holds slots for
        // that no plan covers is a plan to author.
        let monday = domain::planner::commencing(now.date);
        let has_slots = (0..7)
            .filter_map(|offset| monday.checked_add(jiff::Span::new().days(offset)).ok())
            .any(|date| !diary.ordinary_slots_of(date).is_empty());
        if has_slots {
            output::no_plan_covers(monday);
        } else {
            output::no_next_slot(now.date);
        }
        return Ok(());
    }
    output::microcycle(sessions);
    println!();

    // 4. The first still to be prescribed, which is the one delivered.
    let Some(next) = sessions
        .iter()
        .find(|session| session.state == SessionState::ToBePrescribed)
    else {
        let after = sessions
            .last()
            .and_then(|last| diary.first_ordinary_after(last.slot.date));
        output::microcycle_complete(after);
        return Ok(());
    };

    output::next_slot(next);
    println!();
    run(next.slot, database, zone, credentials).await
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

/// Collect and derive one discipline's record.
///
/// **An unreachable source is reported and stepped past**, as `gym strength`
/// does: what is already landed still answers, just less recently. Deriving
/// is not stepped past, because it contacts nothing.
async fn collect(
    discipline: Discipline,
    database: &Path,
    zone: &OperatorZone,
    credentials: &infrastructure::Credentials,
) -> Result<(), Failure> {
    let known = known(discipline)?.collects();
    let stream = known
        .landing_stream()
        .map_err(|error| Failure::usage(&error))?;

    output::run_started(&stream);
    let collected = match crate::source_access(known, None, credentials) {
        Ok(access) => wiring::run(Command::Extract(access), known, database)
            .await
            .map_err(Failure::from),
        Err(failure) => Err(failure),
    };
    match collected {
        Ok(outcome) => gym::report(&stream, outcome),
        Err(failure) => output::not_collected(&stream, failure.message_text()),
    }

    output::derivation_started(&stream);
    let derived = wiring::run(Command::Normalise(zone.clone()), known, database).await?;
    gym::report(&stream, derived);
    Ok(())
}

/// The next slot's discipline's own `next`, from the slot's date.
///
/// **An arm per discipline**, because the two take different inputs: the gym
/// collects, derives, prescribes and delivers, and cycling reads its authored
/// programme and delivers. A third discipline is an entry and an arm.
async fn run(
    next: ScheduledSlot,
    database: &Path,
    zone: &OperatorZone,
    credentials: &infrastructure::Credentials,
) -> Result<(), Failure> {
    let discipline = known(next.discipline)?;
    match next.discipline {
        Discipline::Gym => {
            let access = crate::source_access(discipline.collects(), None, credentials)?;
            gym::next(
                discipline,
                database,
                zone,
                Some(&next.date.to_string()),
                access,
                credentials,
            )
            .await
        }
        // A credential that is absent costs the delivery and not the answer, as
        // it does for `cycling next` — and says so, rather than printing a
        // session that looks delivered (#184).
        Discipline::Cycling => {
            let peloton = plan::peloton(credentials);
            let to = crate::to_peloton(&peloton);
            cycling::next(database, zone, next.date, None, to).await
        }
    }
}

fn known(discipline: Discipline) -> Result<&'static catalogue::KnownDiscipline, Failure> {
    catalogue::discipline(discipline.as_str()).ok_or_else(|| {
        Failure::message(
            format!("this build has no daily loop for {discipline}"),
            exit::USAGE,
        )
    })
}
