//! `next`, over this build's adapters.
//!
//! **The use case is `application::next`** (#397); what is here is which
//! adapters it runs over, so `cli` and `web` start the same run and neither
//! composes it for itself (#398).

use std::path::Path;

use application::{
    StoreError,
    commit::{CommitPorts, Committing},
    cycling::{Riding, RidingPorts},
    microcycle::{Microcycle, MicrocyclePorts},
    next::{
        CanonicalPorts, CyclingPorts, GymPorts, Next, NextError, NextPorts, NoAccess, Progress,
        Report,
    },
};
use domain::{
    normalised::OperatorZone,
    prescription::{DestinationName, InvalidDelivery},
    schedule::{DayPart, Discipline},
};
use infrastructure::{
    FileRunLock, SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore,
    SqliteCyclingDeliveryStore, SqliteCyclingMesocycleStore, SqliteDiaryStore,
    SqliteExerciseHistory, SqliteFtpHistory, SqliteGenerationParameterStore,
    SqliteGymMesocycleStore, SqliteNormalisedCyclingSessionReader,
    SqliteNormalisedGymSessionReader, SqlitePelotonClassStore, SqlitePlanStore,
    SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore, connect,
    peloton::{
        PelotonClassSource, PelotonHoldingRides, PelotonProgrammes, PelotonRideDestination,
        catalogue::ClassCatalogue,
    },
};

use crate::{
    access, catalogue,
    config::{SourceAccess, source_access_for},
    holidays::Calendars,
    wiring::{self, WiringError},
};

/// Why `next` could not be started, or could not answer.
#[derive(Debug, thiserror::Error)]
pub enum NextFailure {
    #[error("this build has no daily loop for {discipline}")]
    NoLoop { discipline: Discipline },
    #[error(transparent)]
    Destination(#[from] InvalidDelivery),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Wiring(#[from] WiringError),
    #[error(transparent)]
    Next(#[from] NextError),
}

/// Collect everything, say where the plan stands, and deliver what is next.
///
/// `skip` takes what is left of the current session's window first, for the
/// reason given (#391). Progress is reported as each step runs.
///
/// # Errors
///
/// [`NextFailure`] if another run of `next` is going, if the store is
/// unavailable, if no week has been recorded, or if a skip cannot be taken. A
/// source that cannot be reached is a failed step in the report and nothing
/// more (§ 36).
pub async fn run(
    database: &Path,
    zone: &OperatorZone,
    credentials: &infrastructure::Credentials,
    now: DayPart,
    skip: Option<&str>,
    progress: &(impl Progress + Sync),
) -> Result<Report, NextFailure> {
    let pool = connect(database).await?;

    // **This build collects the gym from Hevy and cycling from Peloton**, and
    // the catalogue says so: a second source for either is an entry there and
    // an arm in `wiring`.
    let access = |discipline| -> Result<Result<SourceAccess, NoAccess>, NextFailure> {
        let stream = known(discipline)?.collects();
        let Some(system) = stream.system() else {
            return Ok(Err(NoAccess {
                detail: format!("{} is read from a folder, not a system", stream.name()),
            }));
        };
        Ok(
            source_access_for(system, None, credentials).map_err(|error| NoAccess {
                detail: error.to_string(),
            }),
        )
    };
    let gym_records =
        wiring::hevy_records(access(Discipline::Gym)?, database, zone.clone()).await?;
    let cycling_records =
        wiring::peloton_records(access(Discipline::Cycling)?, database, zone.clone()).await?;

    // **A credential that is absent costs what needs it, and nothing else.**
    // The catalogue answers choosing a class and building a mesocycle without
    // Peloton; refreshing it and delivering to the stack are what it costs.
    let unreachable = |error: &access::AccessError| NoAccess {
        detail: error.to_string(),
    };
    let peloton = access::peloton(credentials);
    let stored = SqlitePelotonClassStore::new(pool.clone());
    let classes = peloton.as_ref().ok().map(|peloton| &peloton.classes);
    let catalogue = ClassCatalogue::over(&stored, classes);
    let class_source = peloton
        .as_ref()
        .map(|peloton| PelotonClassSource::new(&peloton.classes, &stored))
        .map_err(unreachable);
    let ride_destination = peloton
        .as_ref()
        .map(|peloton| PelotonRideDestination::new(&peloton.classes, &peloton.stack))
        .map_err(unreachable);
    let gym_destination = access::hevy_routines(known(Discipline::Gym)?.delivers_to(), credentials)
        .map_err(|error| unreachable(&error));

    let plans = SqlitePlanStore::new(pool.clone(), zone.clone());
    let parameters = SqliteGenerationParameterStore::new(pool.clone());
    let deliveries = SqlitePrescriptionDeliveryStore::new(pool.clone());
    let next = Next::new(NextPorts {
        gym_records,
        cycling_records,
        canonical: CanonicalPorts {
            normalised_gym: SqliteNormalisedGymSessionReader::new(pool.clone()),
            canonical_gym: SqliteCanonicalGymSessionStore::new(pool.clone()),
            normalised_cycling: SqliteNormalisedCyclingSessionReader::new(pool.clone()),
            canonical_cycling: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
        },
        classes: class_source,
        holidays: Calendars,
        diary: SqliteDiaryStore::new(pool.clone()),
        plans: plans.clone(),
        microcycle: Microcycle::new(
            MicrocyclePorts {
                diary: SqliteDiaryStore::new(pool.clone()),
                plans: plans.clone(),
                gym_deliveries: deliveries.clone(),
                cycling_deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
                gym_performed: SqliteCanonicalGymSessionStore::new(pool.clone()),
                cycling_performed: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
            },
            destination(Discipline::Gym)?,
            destination(Discipline::Cycling)?,
        ),
        committing: Committing::new(
            CommitPorts {
                plans: plans.clone(),
                parameters: parameters.clone(),
                holding: PelotonHoldingRides::new(catalogue),
                ridden: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
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
            riding: riding(&pool, zone, catalogue)?,
            destination: ride_destination,
        },
        // Beside the store, as extraction's are: a copy of the store elsewhere
        // is a separate system with its own lock.
        lock: FileRunLock::beside(database),
    });

    Ok(next.run(now, skip, progress).await?)
}

/// The cycling use case, over this build's stores and catalogue.
///
/// # Errors
///
/// [`NextFailure`] if the catalogue names no destination for cycling.
pub fn riding<'a>(
    pool: &infrastructure::SqlitePool,
    zone: &OperatorZone,
    catalogue: ClassCatalogue<'a>,
) -> Result<
    Riding<
        SqliteCyclingMesocycleStore,
        SqlitePlanStore,
        PelotonHoldingRides<'a>,
        SqliteCanonicalCyclingSessionStore,
        SqliteFtpHistory,
        SqliteCyclingDeliveryStore,
    >,
    NextFailure,
> {
    Ok(Riding::new(
        RidingPorts {
            mesocycles: SqliteCyclingMesocycleStore::new(pool.clone()),
            plans: SqlitePlanStore::new(pool.clone(), zone.clone()),
            holding: PelotonHoldingRides::new(catalogue),
            ridden: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
            ftp: SqliteFtpHistory::new(pool.clone()),
            deliveries: SqliteCyclingDeliveryStore::new(pool.clone()),
        },
        destination(Discipline::Cycling)?,
    ))
}

/// Where a discipline's sessions are put, as the catalogue names it.
///
/// # Errors
///
/// [`NextFailure`] if this build has no loop for the discipline, or its
/// destination's name is not one.
pub fn destination(discipline: Discipline) -> Result<DestinationName, NextFailure> {
    Ok(DestinationName::try_from(
        known(discipline)?.delivers_to().name().to_owned(),
    )?)
}

fn known(discipline: Discipline) -> Result<&'static catalogue::KnownDiscipline, NextFailure> {
    catalogue::discipline(discipline.as_str()).ok_or(NextFailure::NoLoop { discipline })
}
