//! The prescription commands.
//!
//! Kept apart from the stream commands because prescription is not a stream:
//! the catalogue is one entry per thing this build can *collect*, and generation
//! collects nothing. There is no `--source`, no credential and no run lock.

use std::path::Path;

use application::{
    ProgrammeAuthor as _, WorkoutPrescriber as _,
    prescribe::{Authoring, Prescribing, PrescriptionPorts},
};
use domain::gym::OperatorZone;
use infrastructure::{
    Document, SqliteExerciseHistory, SqliteGenerationParameterStore, SqlitePrescribedWorkoutStore,
    SqliteProgrammeStore, connect,
};
use jiff::civil::Date;

use crate::{Failure, config, config::ConfigError, exit, output};

/// Read a document and store the programme it describes.
pub async fn author(database: &Path, zone: &OperatorZone, path: &Path) -> Result<(), Failure> {
    let document = Document::read(path).map_err(|error| Failure::usage(&error))?;
    let parameters = document
        .parameters()
        .map_err(|error| Failure::usage(&error))?;
    let programme = document
        .programme(&parameters, zone.as_time_zone())
        .map_err(|error| Failure::usage(&error))?;

    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let id = Authoring::new(
        SqliteProgrammeStore::new(pool.clone(), zone.clone()),
        SqliteGenerationParameterStore::new(pool),
    )
    .author(&programme, &parameters)
    .await
    .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::programme_authored(id, &programme, &parameters);
    Ok(())
}

/// Report the programme in force and where its ladder stands.
///
/// **Reads and prints, and issues nothing.** Asking where the ladder is should not
/// put a prescription in the store — a report that changed what it reports on is
/// worse than no report.
pub async fn standing(database: &Path, zone: &OperatorZone) -> Result<(), Failure> {
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let prescriber = Prescribing::new(PrescriptionPorts {
        history: SqliteExerciseHistory::new(pool.clone()),
        programmes: SqliteProgrammeStore::new(pool.clone(), zone.clone()),
        parameters: SqliteGenerationParameterStore::new(pool.clone()),
        prescriptions: SqlitePrescribedWorkoutStore::new(pool, zone.id().to_owned()),
    });

    // Today in the operator's zone, because "where the ladder stands" is a
    // question about now and the answer moves at local midnight.
    let today = jiff::Timestamp::now().to_zoned(zone.as_time_zone()).date();
    let standing = prescriber
        .standing(today)
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
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let programmes = SqliteProgrammeStore::new(pool.clone(), zone.clone());
    let prescriber = Prescribing::new(PrescriptionPorts {
        history: SqliteExerciseHistory::new(pool.clone()),
        programmes: SqliteProgrammeStore::new(pool.clone(), zone.clone()),
        parameters: SqliteGenerationParameterStore::new(pool.clone()),
        prescriptions: SqlitePrescribedWorkoutStore::new(pool.clone(), zone.id().to_owned()),
    });

    let date = resolve(&programmes, date).await?;
    let issued = prescriber
        .prescribe(date)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::prescription(&issued);
    Ok(())
}

/// The date to prescribe for.
///
/// **The defaulting itself is [`config::date`]**, which takes the calendar and
/// the clock and is unit-tested. What is left here is the part that needs the
/// store: a default is relative to the programme in force, so the programme has
/// to be read before the date can be worked out.
async fn resolve(programmes: &SqliteProgrammeStore, given: Option<&str>) -> Result<Date, Failure> {
    let current = application::ProgrammeStore::current(programmes)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let Some((_, programme)) = current else {
        return Err(Failure::message(
            application::PrescriptionError::NoProgramme.to_string(),
            exit::STORE,
        ));
    };

    config::date(given, programme.calendar(), jiff::Timestamp::now()).map_err(|error| match error {
        // A date that will not parse is the operator's typing; a block with no
        // session left is the store's state. They exit differently.
        ConfigError::NotADate { .. } => Failure::usage(&error),
        _ => Failure::message(error.to_string(), exit::STORE),
    })
}
