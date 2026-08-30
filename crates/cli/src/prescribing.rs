//! The prescription commands.
//!
//! Kept apart from the stream commands because prescription is not a stream:
//! the catalogue is one entry per thing this build can *collect*, and generation
//! collects nothing. There is no `--source`, no credential and no run lock.

use std::path::Path;

use application::{
    DiaryStore as _, GenerationParameterStore as _, PrescriptionDeliverer as _,
    ProgrammeAuthor as _, ProgrammeStore as _, WorkoutPrescriber as _,
    compare::{Comparing, ComparisonPorts},
    deliver::{Delivering, DeliveryPorts},
    prescribe::{Authoring, Prescribing, PrescriptionPorts},
};
use domain::{
    gym::OperatorZone,
    prescription::{Programme, Skip},
    schedule::Discipline,
};
use infrastructure::{
    Document, HevyRoutinePreview, HevyRoutines, SqliteDiaryStore, SqliteExerciseHistory,
    SqliteGenerationParameterStore, SqlitePerformedWorkoutReader, SqlitePrescribedWorkoutStore,
    SqlitePrescriptionDeliveryStore, SqliteProgrammeStore, connect,
};
use jiff::civil::Date;

use crate::{Failure, catalogue, config, config::ConfigError, exit, output};

/// Read a document and store the programme it describes.
pub async fn add(database: &Path, zone: &OperatorZone, path: &Path) -> Result<(), Failure> {
    let document = Document::read(path).map_err(|error| Failure::usage(&error))?;
    let stated = document
        .parameters()
        .map_err(|error| Failure::usage(&error))?;

    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let programmes = SqliteProgrammeStore::new(pool.clone(), zone.clone());
    let parameter_store = SqliteGenerationParameterStore::new(pool.clone());

    // **A document without a `[parameters]` section is authored against the set
    // in force.** § 14 asks only for the current value of a generation
    // parameter, and what each prescription was generated against is recorded on
    // the prescription — so restating them is how they are changed, not a
    // condition of authoring. A test is two sessions and has nothing to say
    // about a warm-up ramp.
    let parameters = match stated {
        Some(parameters) => parameters,
        None => parameter_store
            .current()
            .await
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
            .map(|(_, parameters)| parameters)
            .ok_or_else(|| {
                Failure::usage(
                    &"this document states no parameters and none are stored: \
                      the first programme authored has to carry them",
                )
            })?,
    };

    // **A test's fills are resolved here, against the store, once.** The
    // document names what changes and the programme before it supplies the rest
    // (decision 0013), and doing that at authoring rather than at derivation is
    // what keeps the stored test complete on its own — so correcting the
    // predecessor later cannot silently move what this test prescribes.
    let inherited = if document.inherits() {
        let start = document.start().map_err(|error| Failure::usage(&error))?;
        programmes
            .preceding(start)
            .await
            .map_err(|error| Failure::message(error.to_string(), exit::STORE))?
            .map(|(_, programme)| programme)
    } else {
        None
    };
    // **The days the gym loses, worked out here and recorded.** The schedule
    // knows when there is room to train and which slots are the gym's; the
    // programme is told its window and reads back what it loses. Resolved at
    // authoring for the same reason a test's fills are — the stored programme is
    // then complete on its own, so a holiday coming off the calendar afterwards
    // cannot retroactively move what it prescribed.
    //
    // A document that states its own interruptions overrides this, and is not
    // asked. That is the case where the diary has not been told something.
    let derived = derived_interruptions(&document, &SqliteDiaryStore::new(pool.clone())).await?;

    let programme = document
        .programme(
            &parameters,
            zone.as_time_zone(),
            inherited.as_ref().map(Programme::fills),
            &derived,
        )
        .map_err(|error| Failure::usage(&error))?;

    let (id, authored) = Authoring::new(programmes, parameter_store)
        .author(&programme, &parameters)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    output::programme_authored(id, authored, &programme, &parameters);
    Ok(())
}

/// The days this programme's window loses, from the schedule.
///
/// Empty where nothing has been recorded about the operator's week, which is a
/// machine that has not run `fitness schedule add` yet — not a claim that the
/// block runs through everything.
async fn derived_interruptions(
    document: &Document,
    diary: &SqliteDiaryStore,
) -> Result<Vec<Skip>, Failure> {
    let Some(window) = document.window().map_err(|error| Failure::usage(&error))? else {
        return Ok(Vec::new());
    };

    let diary = diary
        .diary()
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    Ok(diary
        .unavailable(window.0, window.1, Discipline::Gym)
        .into_iter()
        .map(Skip::day)
        .collect())
}

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
        programmes: SqliteProgrammeStore::new(pool.clone(), zone.clone()),
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

    let programmes = SqliteProgrammeStore::new(pool.clone(), zone.clone());
    let prescriber = Prescribing::new(PrescriptionPorts {
        history: SqliteExerciseHistory::new(pool.clone()),
        programmes: SqliteProgrammeStore::new(pool.clone(), zone.clone()),
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
/// **The defaulting itself is [`config::date`]**, which takes the calendar and
/// the clock and is unit-tested. What is left here is the part that needs the
/// store.
///
/// **A named date needs no programme.** Programmes succeed one another, so
/// which one covers that date is settled when the prescription is derived —
/// and asking the store first would refuse a perfectly good date merely because
/// nothing is planned for *today*.
async fn resolve(
    programmes: &SqliteProgrammeStore,
    zone: &OperatorZone,
    given: Option<&str>,
) -> Result<Date, Failure> {
    if let Some(text) = given {
        return config::named_date(text).map_err(|error| Failure::usage(&error));
    }

    let now = jiff::Timestamp::now();
    let today = now.to_zoned(zone.as_time_zone()).date();
    let covering = application::ProgrammeStore::on(programmes, today)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;
    let Some((_, programme)) = covering else {
        return Err(Failure::message(
            application::PrescriptionError::NoProgramme { date: today }.to_string(),
            exit::STORE,
        ));
    };

    config::date(None, programme.calendar(), now).map_err(|error| match error {
        // A date that will not parse is the operator's typing; a block with no
        // session left is the store's state. They exit differently.
        ConfigError::NotADate { .. } => Failure::usage(&error),
        _ => Failure::message(error.to_string(), exit::STORE),
    })
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
    credentials: &infrastructure::Credentials,
) -> Result<(), Failure> {
    let pool = connect(database)
        .await
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let programmes = SqliteProgrammeStore::new(pool.clone(), zone.clone());
    let date = resolve(&programmes, zone, date).await?;

    let prescriptions = SqlitePrescribedWorkoutStore::new(pool.clone(), zone.id().to_owned());
    if preview {
        return preview_delivery(
            prescriptions,
            SqliteProgrammeStore::new(pool.clone(), zone.clone()),
            date,
        )
        .await;
    }

    let known = catalogue::source("hevy")
        .ok_or_else(|| Failure::message("this build has no hevy destination wired", exit::USAGE))?;
    let base_url = std::env::var(known.base_url_variable())
        .unwrap_or_else(|_| known.default_base_url().to_owned());
    let access = config::SourceAccess::resolve(
        known,
        base_url,
        std::env::var(known.api_key_variable()),
        credentials.key(known.name()),
    )
    .map_err(|error| Failure::usage(&error))?;

    let destination = HevyRoutines::new(access.base_url, access.api_key)
        .map_err(|error| Failure::message(error.to_string(), exit::STORE))?;

    let delivering = Delivering::new(DeliveryPorts {
        prescriptions,
        programmes: SqliteProgrammeStore::new(pool.clone(), zone.clone()),
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
    programmes: SqliteProgrammeStore,
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

    let programmes = SqliteProgrammeStore::new(pool.clone(), zone.clone());
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
