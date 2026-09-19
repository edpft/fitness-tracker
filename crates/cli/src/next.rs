//! `fitness next` — what's next, from what should happen and what has.
//!
//! The operator, 2026-09-19: *"This command is all about what's next, based on
//! what has happened and what should happen."* (#137)
//!
//! **What should happen is the diary.** Its slots, with every alteration
//! applied, say which discipline has each one — so the schedule alone knows
//! whose session is next, and no discipline is asked.
//!
//! **What has happened is the record.** The slot before today, and any slot
//! today, is accounted for by a performed session of its discipline on or after
//! its date; see [`domain::schedule::unaccounted`]. Where the store holds none,
//! that discipline's record is collected and derived before looking again,
//! since the session may be at the source and not yet landed.
//!
//! **A slot still unaccounted for is reported and nothing more.** Recording
//! why it was missed, and moving anything because of it, is #177.
//!
//! Then the next slot's discipline runs its own `next`, from that slot's date,
//! and prints exactly what it prints alone.

use std::{collections::BTreeMap, path::Path};

use application::{DiaryStore as _, PerformedSessionLog as _};
use domain::{
    normalised::OperatorZone,
    schedule::{Discipline, ScheduledSlot, unaccounted},
};
use infrastructure::{
    SqliteCyclingSessionLog, SqliteDiaryStore, SqlitePerformedWorkoutReader, connect,
};
use jiff::civil::Date;

use crate::{
    Failure, catalogue, config, cycling, exit, gym, output, plan, wiring, wiring::Command,
};

/// Account for what was due, then run the next slot's discipline.
///
/// # Errors
///
/// [`Failure`] if the store is unavailable, if no week has been recorded, or
/// if the next discipline's own `next` fails. A source that cannot be reached
/// while accounting is reported and stepped past (§ 36): the record already
/// landed still answers, and the next session is still worth delivering.
pub async fn next(
    database: &Path,
    zone: &OperatorZone,
    date: Option<&str>,
    credentials: &infrastructure::Credentials,
) -> Result<(), Failure> {
    let today = match date {
        Some(text) => config::named_date(text).map_err(|error| Failure::usage(&error))?,
        None => jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date(),
    };

    let pool = connect(database).await?;
    let diary = SqliteDiaryStore::new(pool.clone()).diary().await?;
    if diary.patterns().is_empty() {
        return Err(Failure::message(
            "no week has been recorded, so nothing says what is next. \
             Record one first: fitness schedule add",
            exit::USAGE,
        ));
    }

    // 1. What has happened, against what should have.
    let due = diary.due_by(today);
    let mut missing = unaccounted(&due, &performed(&pool, &due, today).await?);

    let uncollected: Vec<Discipline> = {
        let mut disciplines: Vec<Discipline> = missing.iter().map(|slot| slot.discipline).collect();
        disciplines.sort();
        disciplines.dedup();
        disciplines
    };
    if !uncollected.is_empty() {
        for discipline in &uncollected {
            collect(*discipline, database, zone, credentials).await?;
        }
        println!();
        missing = unaccounted(&due, &performed(&pool, &due, today).await?);
    }
    pool.close().await;

    for slot in missing.iter().filter(|slot| slot.date < today) {
        output::unperformed(*slot);
    }

    // 2. What should happen next: a slot today not yet done, or else the first
    //    one after it.
    let Some(next) = missing
        .iter()
        .find(|slot| slot.date == today)
        .copied()
        .or_else(|| diary.first_after(today))
    else {
        output::no_next_slot(today);
        return Ok(());
    };
    output::next_slot(next);
    println!();

    run(next, database, zone, credentials).await
}

/// Each due discipline's session dates, from the earliest due slot to today.
async fn performed(
    pool: &infrastructure::SqlitePool,
    due: &[ScheduledSlot],
    today: Date,
) -> Result<BTreeMap<Discipline, Vec<Date>>, Failure> {
    let mut performed = BTreeMap::new();
    let Some(from) = due.iter().map(|slot| slot.date).min() else {
        return Ok(performed);
    };
    for slot in due {
        if performed.contains_key(&slot.discipline) {
            continue;
        }
        let dates = match slot.discipline {
            Discipline::Gym => {
                SqlitePerformedWorkoutReader::new(pool.clone())
                    .dates_between(from, today)
                    .await?
            }
            Discipline::Cycling => {
                SqliteCyclingSessionLog::new(pool.clone())
                    .dates_between(from, today)
                    .await?
            }
        };
        performed.insert(slot.discipline, dates);
    }
    Ok(performed)
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
        // Credentials that are absent cost the delivery and not the answer, as
        // they do for `cycling next`.
        Discipline::Cycling => {
            let peloton = plan::peloton().ok();
            let to = peloton.as_ref().map(|(classes, stack)| (classes, stack));
            cycling::next(database, next.date, None, to).await
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
