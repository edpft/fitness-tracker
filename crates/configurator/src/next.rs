//! `next`, over this build's adapters.
//!
//! **The use case is `application::next`** (#397); what is here is which
//! adapters it runs over, so `cli` and `web` start the same run and neither
//! composes it for itself (#398).

use std::path::{Path, PathBuf};

use application::{
    Build, DerivedFrom, NormalisationError, NormalisationSummary, NormalisedLayers, StoreError,
    commit::{CommitPorts, Committing},
    cycling::{Riding, RidingPorts},
    microcycle::{Microcycle, MicrocyclePorts},
    next::{
        CanonicalPorts, CyclingPorts, GymPorts, Next, NextError, NextPorts, NoAccess, Progress,
        Report,
    },
};
use domain::{
    landing::LandingStream,
    normalised::OperatorZone,
    prescription::{DestinationName, InvalidDelivery},
    schedule::{DayPart, Discipline},
};
use infrastructure::{
    FileRunLock, SqliteCanonicalCyclingSessionStore, SqliteCanonicalGymSessionStore,
    SqliteCanonicalWeighInStore, SqliteCyclingDeliveryStore, SqliteCyclingMesocycleStore,
    SqliteDerivationLedger, SqliteDiaryStore, SqliteExerciseHistory, SqliteFtpHistory,
    SqliteGenerationParameterStore, SqliteGymMesocycleStore, SqliteNormalisedCyclingSessionReader,
    SqliteNormalisedGymSessionReader, SqliteNormalisedWeighInReader, SqlitePelotonClassStore,
    SqlitePlanStore, SqlitePrescribedWorkoutStore, SqlitePrescriptionDeliveryStore, connect,
    peloton::{
        PelotonClassSource, PelotonHoldingRides, PelotonProgrammes, PelotonRideDestination,
        catalogue::ClassCatalogue,
    },
};

use crate::{
    access, catalogue,
    config::{SourceAccess, source_access_for},
    holidays::Calendars,
    wiring::{self, Command, Outcome, Unreported, WiringError},
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
    let gym = access_to(known(Discipline::Gym)?.collects(), credentials);
    let gym_records = wiring::hevy_records(gym, database).await?;
    let cycling = access_to(known(Discipline::Cycling)?.collects(), credentials);
    let cycling_records = wiring::peloton_records(cycling, database).await?;
    let weigh_in_records =
        wiring::withings_records(stream_access(WEIGH_INS, credentials), database).await?;
    let (watch_records, watch_days) =
        wiring::garmin_records(stream_access(WATCH, credentials), database).await?;

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
        weigh_in_records,
        watch_records,
        watch_days,
        normalised: Layers {
            database: database.to_path_buf(),
            zone: zone.clone(),
        },
        canonical: CanonicalPorts {
            normalised_gym: SqliteNormalisedGymSessionReader::new(pool.clone()),
            canonical_gym: SqliteCanonicalGymSessionStore::new(pool.clone()),
            normalised_cycling: SqliteNormalisedCyclingSessionReader::new(pool.clone()),
            canonical_cycling: SqliteCanonicalCyclingSessionStore::new(pool.clone()),
            normalised_weigh_ins: SqliteNormalisedWeighInReader::new(pool.clone()),
            canonical_weigh_ins: SqliteCanonicalWeighInStore::new(pool.clone()),
        },
        ledger: SqliteDerivationLedger::new(pool.clone()),
        build: build(BUILD),
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

    // **Boxed, because it holds every source's fetch at once** (#468).
    Ok(Box::pin(next.run(now, skip, progress)).await?)
}

/// The commit this build was made from, as the flake bakes it in.
///
/// **`None` outside nix**, where nothing says which commit is running: such a
/// build derives every layer every run, since it cannot tell whether the code
/// that derived them has changed (#468).
pub const BUILD: Option<&str> = option_env!("FITNESS_BUILD");

/// The build, where it names one commit.
///
/// **A dirty tree or one with no git names none**: the flake says
/// `abc1234-dirty` for every uncommitted change on top of `abc1234`, and
/// `unknown` for every build from outside git, so each stands for many
/// versions of the code and would pass one off as another.
fn build(baked: Option<&str>) -> Option<Build> {
    let baked = baked?;
    let revision = baked.split_whitespace().next()?;
    (revision != "unknown" && !revision.ends_with("-dirty")).then(|| Build::from(baked.to_owned()))
}

/// Every stream this build derives, each through its own arm of
/// [`wiring::run`], which is also how one is derived from the command line.
struct Layers {
    database: PathBuf,
    zone: OperatorZone,
}

impl Layers {
    fn known(stream: &LandingStream) -> Result<&'static catalogue::KnownStream, WiringError> {
        catalogue::lookup(&stream.to_string()).ok_or_else(|| WiringError::Unwired {
            stream: stream.to_string(),
        })
    }
}

/// A failure to read what a layer is derived from is the store's.
fn unavailable(error: &WiringError) -> StoreError {
    match error {
        WiringError::Store(error) => error.clone(),
        other => StoreError::Unavailable {
            detail: other.to_string(),
        },
    }
}

impl NormalisedLayers for Layers {
    fn streams(&self) -> Vec<LandingStream> {
        catalogue::KNOWN
            .iter()
            .filter(|known| wiring::derives(known))
            .filter_map(|known| known.landing_stream().ok())
            .collect()
    }

    async fn inputs(&self, stream: &LandingStream) -> Result<DerivedFrom, StoreError> {
        let known = Self::known(stream).map_err(|error| unavailable(&error))?;
        wiring::inputs(known, &self.database, &self.zone)
            .await
            .map_err(|error| unavailable(&error))
    }

    async fn normalise(
        &self,
        stream: &LandingStream,
    ) -> Result<NormalisationSummary, NormalisationError> {
        let failed = |error: &WiringError| match error {
            WiringError::Normalisation(error) => error.clone(),
            other => NormalisationError::Store(unavailable(other)),
        };
        let known = Self::known(stream).map_err(|error| failed(&error))?;
        match wiring::run(
            Command::Normalise(self.zone.clone()),
            known,
            &self.database,
            &Unreported,
        )
        .await
        {
            Ok(Outcome::Derived(summary)) => Ok(*summary),
            Ok(_) => Err(NormalisationError::Store(StoreError::Unavailable {
                detail: format!("deriving {stream} answered something other than a derivation"),
            })),
            Err(error) => Err(failed(&error)),
        }
    }
}

/// How a stream `next` collects is reached: its system's credential and root.
///
/// **No access is a step that fails, not a run that does** (§ 36): a stream
/// read from a folder, or a source with no credential, is reported and passed.
fn access_to(
    stream: &catalogue::KnownStream,
    credentials: &infrastructure::Credentials,
) -> Result<SourceAccess, NoAccess> {
    let system = stream.system().ok_or_else(|| NoAccess {
        detail: format!("{} is read from a folder, not a system", stream.name()),
    })?;
    source_access_for(system, None, credentials).map_err(|error| NoAccess {
        detail: error.to_string(),
    })
}

/// How an entry `next` collects that no discipline does is reached, by its
/// name: body weight, and what the watch records.
fn stream_access(
    name: &str,
    credentials: &infrastructure::Credentials,
) -> Result<SourceAccess, NoAccess> {
    let stream = catalogue::lookup(name).ok_or_else(|| NoAccess {
        detail: format!("this build does not collect {name}"),
    })?;
    access_to(stream, credentials)
}

/// Where body weight comes from.
pub const WEIGH_INS: &str = "withings.measurements";

/// What the watch records is collected under (#432): its activities every run,
/// and its days and nights until yesterday is whole.
pub const WATCH: &str = "garmin.activities";

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

#[cfg(test)]
mod tests {
    use application::Build;

    use super::build;

    /// **Only a clean commit is a build**, since `-dirty` and `unknown` each
    /// name many versions of the code.
    #[test]
    fn only_a_clean_commit_names_a_build() {
        assert_eq!(
            build(Some("5734906 (2026-10-18)")),
            Some(Build::from("5734906 (2026-10-18)".to_owned()))
        );
        assert_eq!(build(Some("5734906-dirty (2026-10-18)")), None);
        assert_eq!(build(Some("unknown (0000-00-00)")), None);
        assert_eq!(build(None), None);
    }
}
