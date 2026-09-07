//! The prescription commands.
//!
//! Kept apart from the stream commands because prescription is not a stream:
//! the catalogue is one entry per thing this build can *collect*, and generation
//! collects nothing. There is no `--source`, no credential and no run lock.

use std::path::Path;

use application::{
    GenerationParameterStore as _, PrescriptionDeliverer as _, WorkoutPrescriber as _,
    compare::{Comparing, ComparisonPorts},
    deliver::{Delivering, DeliveryPorts},
    prescribe::{Prescribing, PrescriptionPorts},
};
use domain::gym::OperatorZone;
use infrastructure::{
    HevyRoutinePreview, HevyRoutines, SqliteExerciseHistory, SqliteGenerationParameterStore,
    SqliteGymMesocycleStore, SqlitePerformedWorkoutReader, SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore, connect,
};
use jiff::civil::Date;

use crate::{Failure, catalogue, config, exit, output};

/// Report the parameters every prescription is generated against (§ 14).
///
/// **Only the current set.** Superseded rows stay in the store and nothing
/// reads one — what a prescription was generated against is recorded on the
/// prescription itself, which is what makes that safe.
pub async fn parameters(database: &Path) -> Result<(), Failure> {
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let current = SqliteGenerationParameterStore::new(pool)
        .current()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    match current {
        Some((authored_at, parameters)) => {
            output::parameters_in_force(authored_at, &parameters);
            Ok(())
        }
        // Not an empty report: a store with no parameters can hold a programme
        // and prescribe nothing from it, and saying so is more use than printing
        // a set of headings with nothing under them.
        None => Err(Failure::usage(
            &"this store has no generation parameters. Run `fitness init` — it stores them",
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
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let prescriber = Prescribing::new(PrescriptionPorts {
        history: SqliteExerciseHistory::new(pool.clone()),
        programmes: SqliteGymMesocycleStore::new(pool.clone(), zone.clone()),
        parameters: SqliteGenerationParameterStore::new(pool.clone()),
        prescriptions: SqlitePrescribedWorkoutStore::new(pool.clone(), zone.id().to_owned()),
        lifecycle: SqlitePrescriptionDeliveryStore::new(pool),
    });

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
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let programmes = SqliteGymMesocycleStore::new(pool.clone(), zone.clone());
    let prescriber = Prescribing::new(PrescriptionPorts {
        history: SqliteExerciseHistory::new(pool.clone()),
        programmes: SqliteGymMesocycleStore::new(pool.clone(), zone.clone()),
        parameters: SqliteGenerationParameterStore::new(pool.clone()),
        prescriptions: SqlitePrescribedWorkoutStore::new(pool.clone(), zone.id().to_owned()),
        lifecycle: SqlitePrescriptionDeliveryStore::new(pool.clone()),
    });

    let date = resolve(&programmes, zone, date).await?;
    let issued = prescriber
        .prescribe(date)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::prescription(&issued);
    Ok(())
}

/// The date to prescribe for.
///
/// **The defaulting itself is [`application::prescribe::next_session`]**,
/// which reads the programme in force and asks its calendar. It moved out of
/// this crate on 2026-08-30: a terminal and a browser must not be able to
/// disagree about which session is next, and while it lived here they could.
///
/// What is left is the half that is genuinely a transport's: turning the text an
/// operator typed into a date, and telling a typo apart from a finished block so
/// the two exit differently.
///
/// **A named date needs no programme.** Programmes succeed one another, so which
/// one covers that date is settled when the prescription is derived — and asking
/// the store first would refuse a perfectly good date merely because nothing is
/// planned for *today*.
async fn resolve(
    programmes: &SqliteGymMesocycleStore,
    zone: &OperatorZone,
    given: Option<&str>,
) -> Result<Date, Failure> {
    if let Some(text) = given {
        return config::named_date(text).map_err(|error| Failure::usage(&error));
    }

    application::prescribe::next_session(programmes, jiff::Timestamp::now(), zone)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))
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
    credentials: &infrastructure::Credentials,
) -> Result<(), Failure> {
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let programmes = SqliteGymMesocycleStore::new(pool.clone(), zone.clone());
    let date = resolve(&programmes, zone, date).await?;

    let prescriptions = SqlitePrescribedWorkoutStore::new(pool.clone(), zone.id().to_owned());
    if preview {
        return preview_delivery(
            prescriptions,
            SqliteGymMesocycleStore::new(pool.clone(), zone.clone()),
            date,
        )
        .await;
    }

    // **The destination is passed in rather than named here.** It used to be the
    // string "hevy", which was honest while there was one of them and became a
    // scaling question the moment a second discipline was contemplated: a
    // discipline knows its own sink, so `gym next` hands its one over and the
    // flat command names the build's only one. Neither guesses.
    let base_url = std::env::var(known.base_url_variable())
        .unwrap_or_else(|_| known.default_base_url().to_owned());
    let access = config::SourceAccess::resolve(
        known,
        base_url,
        std::env::var(known.api_key_variable()),
        credentials.key(known.name()),
    )
    .map_err(|error| Failure::usage(&error))?;

    // `resolve` above builds the key-based kind, so the other arm is a
    // contradiction rather than a case: it would mean this source's catalogue
    // entry and the call that read its credential disagree.
    let config::SourceAccess::ApiKey { base_url, api_key } = access else {
        return Err(Failure::message(
            format!("{} is reached with an API key", known.name()),
            exit::STORE,
        ));
    };

    let destination = HevyRoutines::new(base_url, api_key)
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let delivering = Delivering::new(DeliveryPorts {
        prescriptions,
        programmes: SqliteGymMesocycleStore::new(pool.clone(), zone.clone()),
        deliveries: SqlitePrescriptionDeliveryStore::new(pool.clone()),
        destination,
    });

    let delivered = delivering
        .deliver(date)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::delivery(&delivered);
    Ok(())
}

/// The whole path except the two irreversible steps.
///
/// **The store it writes to is thrown away**, so a preview cannot leave a record
/// claiming a session was delivered — which would make the real delivery a
/// no-op and lose the session entirely. The rendering is the real one.
async fn preview_delivery(
    prescriptions: SqlitePrescribedWorkoutStore,
    programmes: SqliteGymMesocycleStore,
    date: Date,
) -> Result<(), Failure> {
    let destination = HevyRoutinePreview::new()
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let delivering = Delivering::new(DeliveryPorts {
        prescriptions,
        programmes,
        deliveries: ForgetfulDeliveries,
        destination: &destination,
    });

    let delivered = delivering
        .deliver(date)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let Some(body) = destination.body() else {
        return Err(Failure::message("nothing was rendered", exit::STORE));
    };

    output::preview(&delivered, &body);
    Ok(())
}

/// A delivery store that remembers nothing.
///
/// Not a test double: it is what makes a preview safe to run against the real
/// store, because the one thing a preview must not do is record a delivery that
/// never happened.
struct ForgetfulDeliveries;

impl application::PrescriptionDeliveryStore for ForgetfulDeliveries {
    async fn reference_for(
        &self,
        _prescription: application::PrescribedWorkoutId,
        _destination: &application::DestinationName,
    ) -> Result<Option<application::DeliveryReference>, application::StoreError> {
        Ok(None)
    }

    /// **Nothing occupies anything, so a preview always renders a first
    /// delivery.** Answering otherwise would send the preview down the
    /// replacement path and have it print what a `PUT` would send — which is
    /// the same bytes, but aimed at a routine this run has no business naming.
    async fn occupying(
        &self,
        _date: jiff::civil::Date,
        _destination: &application::DestinationName,
    ) -> Result<
        Option<(
            application::PrescribedWorkoutId,
            application::DeliveryReference,
        )>,
        application::StoreError,
    > {
        Ok(None)
    }

    async fn record(
        &self,
        _prescription: application::PrescribedWorkoutId,
        _destination: &application::DestinationName,
        _reference: &application::DeliveryReference,
        _at: jiff::Timestamp,
    ) -> Result<(), application::StoreError> {
        Ok(())
    }

    async fn hand_over(
        &self,
        _from: application::PrescribedWorkoutId,
        _to: application::PrescribedWorkoutId,
        _destination: &application::DestinationName,
        _reference: &application::DeliveryReference,
        _at: jiff::Timestamp,
    ) -> Result<(), application::StoreError> {
        Ok(())
    }
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
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let programmes = SqliteGymMesocycleStore::new(pool.clone(), zone.clone());
    let comparing = Comparing::new(ComparisonPorts {
        prescriptions: SqlitePrescribedWorkoutStore::new(pool.clone(), zone.id().to_owned()),
        workouts: SqlitePerformedWorkoutReader::new(pool),
    });

    let date = resolve(&programmes, zone, date).await?;
    let comparison = comparing
        .compare(date)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::comparison(&comparison);
    Ok(())
}
