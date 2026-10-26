//! The prescription commands.
//!
//! Kept apart from the stream commands because prescription is not a stream:
//! the catalogue is one entry per thing this build can *collect*, and generation
//! collects nothing. There is no `--source`, no credential and no run lock.

use std::path::Path;

use application::{
    GenerationParameterStore as _, MesocycleStore, PrescriptionDeliverer as _, ProgrammeStore as _,
    ProgressionStore as _, WorkoutPrescriber as _, prescribe::NextSession,
};
use configurator::{
    prescribing::{self, PreviewFailure},
    store::Store,
};
use domain::normalised::OperatorZone;
use jiff::civil::Date;

use crate::{Failure, catalogue, config, exit, output};

/// Report the versions of the programme and each primary progression a plan
/// made now would take (#192), and today's load scales.
///
/// **Only the latest of each.** Superseded versions stay in the store, and a
/// plan made before one was superseded goes on reading it.
pub async fn parameters(database: &Path) -> Result<(), Failure> {
    let store = Store::open(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let versions = store.parameters();
    let now = jiff::Timestamp::now();
    let failed = |error: application::StoreError| Failure::message(error.to_string(), exit::STORE);

    let programme = versions.programme_at(now).await.map_err(failed)?;
    let linear = versions.linear_at(now).await.map_err(failed)?;
    let sbs = versions.sbs_at(now).await.map_err(failed)?;
    let scales = versions
        .scales_on(jiff::Zoned::now().date())
        .await
        .map_err(failed)?;

    match (programme, linear, sbs) {
        (Some(programme), Some(linear), Some(sbs)) => {
            output::parameters_in_force(
                (programme.0, &programme.1),
                (linear.0, &linear.1),
                (sbs.0, &sbs.1),
                &scales,
            );
            Ok(())
        }
        // Not an empty report: a store with none of these can hold a plan and
        // prescribe nothing from it, and saying so is more use than printing a
        // set of headings with nothing under them.
        _ => Err(Failure::usage(
            &"this store has no programme or progression stored. Run `fitness init` — it \
              stores the first of each",
        )),
    }
}

/// Report the programme in force and where its ladder stands.
///
/// **Reads and prints, and issues nothing.** Asking where the ladder is should not
/// put a prescription in the store — a report that changed what it reports on is
/// worse than no report.
pub async fn standing(
    database: &Path,
    zone: &OperatorZone,
    on: Option<&str>,
) -> Result<(), Failure> {
    let store = Store::open(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let prescriber = prescribing::prescriber(&store, zone).await?;

    // **A date, because programmes succeed one another** (decision 0012). With
    // one programme ever in force "the programme" was unambiguous; with three in
    // the store it is a question about a day, and the operator authoring next
    // month's block wants to look at it before it starts.
    //
    // Today in the operator's zone by default, because that is the question
    // being asked nine times in ten — and the answer moves at local midnight.
    let on = match on {
        Some(date) => date
            .parse::<Date>()
            .map_err(|error| Failure::usage(&error))?,
        None => jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date(),
    };
    let standing = prescriber
        .standing(on)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::programme_standing(&standing);
    Ok(())
}

/// Issue the prescription for a date.
pub async fn prescribe(
    database: &Path,
    zone: &OperatorZone,
    date: Option<&str>,
) -> Result<(), Failure> {
    let store = Store::open(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let programmes = prescribing::programmes(&store, zone).await?;
    let prescriber = prescribing::prescriber(&store, zone).await?;

    let Some(date) = resolve(&programmes, zone, date).await? else {
        return Ok(());
    };
    let issued = prescriber
        .prescribe(date)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::prescription(&issued);
    Ok(())
}

/// The date to prescribe for, or `None` where nothing is planned — which has
/// been said by the time this returns.
///
/// **The defaulting itself is [`application::prescribe::next_session`]**,
/// which finds the first session at or after today. It moved out of this crate
/// on 2026-08-30: a terminal and a browser must not be able to disagree about
/// which session is next, and while it lived here they could.
///
/// What is left is the half that is genuinely a transport's: turning the text an
/// operator typed into a date, and reading the clock for today.
///
/// **A named date needs no programme, and is not searched from.** The plumbing
/// answers for the date it was given, so a day the block skips is refused when
/// the prescription is derived. Only the porcelain searches — see [`next`].
async fn resolve(
    programmes: &(impl MesocycleStore + Sync),
    zone: &OperatorZone,
    given: Option<&str>,
) -> Result<Option<Date>, Failure> {
    if let Some(text) = given {
        return config::named_date(text)
            .map(Some)
            .map_err(|error| Failure::usage(&error));
    }
    search(programmes, today(zone)).await
}

/// The first session at or after a date, for `gym next`.
///
/// **`--date` is where to look from, not the one date to answer for** (#122).
/// The operator, 2026-09-09: *"What I'm asking when I ask for the next
/// prescription to be delivered is for the next prescription to be
/// delivered."* So a day the block skips, a day before it opens and a day after
/// a mesocycle's last session all answer with the session that follows.
///
/// `None` where nothing is planned at or after it, which has been said by the
/// time this returns: it is an answer rather than a failure, so the run exits
/// cleanly.
pub async fn next(
    database: &Path,
    zone: &OperatorZone,
    from: Option<&str>,
) -> Result<Option<Date>, Failure> {
    let store = Store::open(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let programmes = prescribing::programmes(&store, zone).await?;

    let from = match from {
        Some(text) => config::named_date(text).map_err(|error| Failure::usage(&error))?,
        None => today(zone),
    };
    search(&programmes, from).await
}

async fn search(
    programmes: &(impl MesocycleStore + Sync),
    from: Date,
) -> Result<Option<Date>, Failure> {
    let found = application::prescribe::next_session(programmes, from)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    match found {
        NextSession::On(date) => Ok(Some(date)),
        NextSession::NothingPlanned { last } => {
            output::nothing_planned(from, last.as_ref());
            Ok(None)
        }
    }
}

/// Today in the operator's zone, which is the day "the next session" is
/// counted from — and it moves at their midnight, not at UTC's.
fn today(zone: &OperatorZone) -> Date {
    jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date()
}

/// Put the prescription for a date where the operator trains from.
///
/// **Delivery does not issue.** It reads what `prescribe` already put in the
/// store, so a destination being unreachable costs a retry rather than a ladder
/// position, and nothing here can advance a programme.
pub async fn deliver(
    database: &Path,
    zone: &OperatorZone,
    date: Option<&str>,
    preview: bool,
    known: &'static catalogue::KnownSource,
    credentials: &configurator::Credentials,
) -> Result<(), Failure> {
    let store = Store::open(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let programmes = prescribing::programmes(&store, zone).await?;
    let Some(date) = resolve(&programmes, zone, date).await? else {
        return Ok(());
    };

    if preview {
        // **The store it writes to is thrown away**, so a preview cannot leave a
        // record claiming a session was delivered. The rendering is the real one.
        let (delivered, body) =
            prescribing::preview(&store, zone, date)
                .await
                .map_err(|failure| match failure {
                    PreviewFailure::Next(failure) => Failure::from(failure),
                    other => Failure::message(other.to_string(), exit::STORE),
                })?;
        output::preview(&delivered, &body);
        return Ok(());
    }

    let destination = configurator::access::hevy_routines(known, credentials)?;
    let delivering = prescribing::delivering(&store, zone, destination).await?;

    let delivered = delivering
        .deliver(date)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::delivery(&delivered);
    Ok(())
}

/// What a session did against what it was told.
///
/// **Reads and writes nothing.** Both halves are already in the store — the
/// prescription because `prescribe` issued it, the performance because
/// `normalise` derived it — so this contacts no source and records no judgement.
pub async fn compare(
    database: &Path,
    zone: &OperatorZone,
    date: Option<&str>,
) -> Result<(), Failure> {
    let store = Store::open(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let programmes = prescribing::programmes(&store, zone).await?;
    let comparing = prescribing::comparing(&store, zone);

    let Some(date) = resolve(&programmes, zone, date).await? else {
        return Ok(());
    };
    let comparison = comparing
        .compare(date)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::comparison(&comparison);
    Ok(())
}
