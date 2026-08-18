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

use crate::{Failure, exit, output};

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
/// **Defaults forward rather than to today.** "The next session" is what an
/// operator wants on a rest day and today is what they want on a training day,
/// and the next programmed day at or after today gives both. It is printed, so
/// the default is never silent.
async fn resolve(programmes: &SqliteProgrammeStore, given: Option<&str>) -> Result<Date, Failure> {
    if let Some(text) = given {
        return text
            .parse::<Date>()
            .map_err(|error| Failure::usage(&format!("{text:?} is not a date: {error}")));
    }

    let current = application::ProgrammeStore::current(programmes)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let Some((_, programme)) = current else {
        return Err(Failure::message(
            application::PrescriptionError::NoProgramme.to_string(),
            exit::STORE,
        ));
    };

    let today = programme.calendar().today(jiff::Timestamp::now());
    programme.calendar().next_programmed(today).ok_or_else(|| {
        Failure::message(
            format!("this programme has no session on or after {today}"),
            exit::STORE,
        )
    })
}
